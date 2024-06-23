// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Handlers for patching manifests.

use crate::Manifest;
use anyhow::Result;
use cargo_metadata::Package;
use toml_edit::DocumentMut;

/// The working version of sp-wasm-interface.
pub const GP_RUNTIME_INTERFACE_VERSION: &str = "19.0.0-pre.1";

/// The working version of gp-externalities.
pub const GP_EXTERNALITIES_VERSION: &str = "0.21.0-pre.1";

/// Get the crates-io name of the provided package.
pub fn crates_io_name(pkg: &str) -> &str {
    // `gear-core-processor` is taken by others, see the docs
    // of [`core-processor::patch_workspace`] for more details.
    if pkg == "gear-core-processor" {
        "core-processor"
    } else {
        pkg
    }
}

/// Patch specified manifest by provided name.
pub fn patch(pkg: &Package) -> Result<Manifest> {
    let mut manifest = Manifest::new(pkg)?;
    let doc = &mut manifest.manifest;

    match manifest.name.as_str() {
        "gear-core-processor" => core_processor::patch(doc),
        "gear-tasks" => runtime_interface::patch(doc),
        "gear-runtime-interface" => runtime_interface::patch(doc),
        "gear-sandbox" => sandbox::patch(doc),
        "gear-sandbox-host" => sandbox_host::patch(doc),
        "gmeta" => gmeta::patch(doc),
        "gmeta-codegen" => gmeta_codegen::patch(doc),
        _ => {}
    }

    Ok(manifest)
}

/// Patch package alias.
pub fn patch_alias(index: &mut Vec<&str>) {
    for (package, alias) in crate::PACKAGE_ALIAS {
        if index.contains(&package) {
            index.push(alias);
        }
    }
}

/// Patch the workspace manifest.
pub fn patch_workspace(name: &str, table: &mut toml_edit::InlineTable) {
    match name {
        "core-processor" | "gear-core-processor" => core_processor::patch_workspace(name, table),
        sub if ["sc-", "sp-", "frame-", "try-runtime-cli"]
            .iter()
            .any(|p| sub.starts_with(p)) =>
        {
            substrate::patch_workspace(name, table)
        }
        _ => {}
    }
}

// Trim the version of dev dependency.
//
// issue: https://github.com/rust-lang/cargo/issues/4242
fn trim_dev_dep(name: &str, manifest: &mut DocumentMut) {
    if let Some(dep) = manifest["dev-dependencies"][name].as_inline_table_mut() {
        dep.remove("workspace");
        dep.insert("version", "~1".into());
    }

    if let Some(dep) = manifest["dev-dependencies"][name].as_table_like_mut() {
        dep.remove("workspace");
        dep.insert("version", toml_edit::value("~1"));
    }
}

/// gear-core-processor handler.
mod core_processor {
    use toml_edit::{DocumentMut, InlineTable};

    /// Pointing the package name of core-processor to
    /// `core-processor` on `crates-io` since this is
    /// the one we own.
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
            // Remove the path definition to point core-processor to
            // crates-io.
            "core-processor" => {
                table.remove("package");
            }
            // Points to `core-processor` for the one on crates-io.
            "gear-core-processor" => {
                table.insert("package", "core-processor".into());
            }
            _ => {}
        }
    }

    /// Patch the manifest of core-processor.
    pub fn patch(manifest: &mut DocumentMut) {
        manifest["package"]["name"] = toml_edit::value("core-processor");
    }
}

/// gmeta handler
mod gmeta {
    use super::trim_dev_dep;
    use toml_edit::DocumentMut;

    /// Patch the manifest of gmetadata.
    pub fn patch(manifest: &mut DocumentMut) {
        trim_dev_dep("gstd", manifest);
        trim_dev_dep("gear-wasm-builder", manifest);
    }
}

/// gmeta handler
mod gmeta_codegen {
    use super::trim_dev_dep;
    use toml_edit::DocumentMut;

    /// Patch the manifest of gmeta.
    pub fn patch(manifest: &mut DocumentMut) {
        trim_dev_dep("gstd", manifest);
        trim_dev_dep("gmeta", manifest);
    }
}

mod runtime_interface {
    use super::GP_RUNTIME_INTERFACE_VERSION;
    use toml_edit::DocumentMut;

    /// Patch the manifest of runtime-interface.
    ///
    /// We need to patch the manifest of package again because
    /// `sp_runtime_interface_proc_macro` includes some hardcode
    /// that could not locate alias packages.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wi) = manifest["dependencies"]["sp-runtime-interface"].as_table_mut() else {
            return;
        };
        wi.insert("version", toml_edit::value(GP_RUNTIME_INTERFACE_VERSION));
        wi.insert("package", toml_edit::value("gp-runtime-interface"));
        wi.remove("workspace");
    }
}

/// sandbox handler.
mod sandbox {
    use toml_edit::DocumentMut;

    /// Replace the wasmi module to the crates-io version.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wasmi) = manifest["dependencies"]["wasmi"].as_inline_table_mut() else {
            return;
        };
        wasmi.insert("package", "gwasmi".into());
        wasmi.insert("version", "0.30.0".into());
        wasmi.remove("branch");
        wasmi.remove("git");
    }
}

/// sandbox_host handler.
mod sandbox_host {
    use toml_edit::DocumentMut;

    /// Replace the wasmi module to the crates-io version.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wasmi) = manifest["dependencies"]["wasmi"].as_inline_table_mut() else {
            return;
        };
        wasmi.insert("version", "0.13.2".into());
        wasmi.remove("branch");
        wasmi.remove("git");
    }
}

/// substrate handler.
mod substrate {
    use super::{GP_EXTERNALITIES_VERSION, GP_RUNTIME_INTERFACE_VERSION};
    use toml_edit::InlineTable;

    /// Patch the substrate packages in the manifest of workspace.
    ///
    /// Substrate packages on crates-io currently have no version management
    /// (<https://github.com/paritytech/polkadot-sdk/issues/2809>),
    /// the following versions are pinned to frame-support-v23.0.0 on crates-io
    /// now, <https://crates.io/crates/frame-system/23.0.0/dependencies> for
    /// the details.
    ///
    /// NOTE: The packages inside of this function are located at
    /// <https://github.com/gear-tech/substrate/tree/cl/v1.1.x-crates-io>.
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
            "frame-support" | "frame-system" | "sp-core" | "sc-client-api" => {
                table.insert("version", "23.0.0".into());
            }
            // matching `frame-support-23.0.0`
            "frame-support-test" => return,
            // matching `sp-core-23.0.0`
            "frame-benchmarking-cli" => {
                table.insert("version", "27.0.0".into());
            }
            // matching `sp-core-23.0.0`
            "sc-cli" => {
                table.insert("version", "0.31.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-state-machine" | "sc-client-db" | "sc-service" => {
                table.insert("version", "0.30.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-api" | "sp-rpc" => {
                table.insert("version", "21.0.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-externalities" => {
                // table.insert("version", "0.21.0".into());
                table.insert("version", GP_EXTERNALITIES_VERSION.into());
                table.insert("package", "gp-externalities".into());
            }
            // matching `frame-support-23.0.0`
            "sp-arithmetic" => {
                table.insert("version", "18.0.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-debug-derive" | "sp-std" => {
                table.insert("version", "10.0.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-io" => {
                table.insert("version", "25.0.0".into());
            }
            // matching `sp-core-23.0.0`
            "sp-runtime" => {
                table.insert("version", "26.0.0".into());
            }
            // matching `sp-runtime-26.0.0`
            "sp-version" => {
                table.insert("version", "24.0.0".into());
            }
            "sp-weights" => {
                table.insert("version", "22.0.0".into());
            }
            "try-runtime-cli" => {
                table.insert("version", "0.32.0".into());
            }
            // sp-allocator is outdated on crates.io, last
            // 3.0.0 forever, here we use gp-allocator instead.
            "sp-allocator" => {
                table.insert("version", "4.1.2".into());
                table.insert("package", "gp-allocator".into());
            }
            // Our sp-wasm-interface is different from the substrate one.
            //
            // ref: sp-wasm-interface-15.0.0
            "sp-wasm-interface" => {
                table.insert("package", "gp-wasm-interface".into());
                table.insert("version", "15.0.0".into());
            }
            // Related to sp-wasm-interface.
            //
            // no ref bcz we own this package.
            "sp-wasm-interface-common" => {
                table.insert("version", "15.0.0".into());
            }
            // Related to sp-wasm-interface.
            //
            // ref:
            // - sp-runtime-interface-18.0.0
            // - sp-runtime-interface-proc-macro-12.0.0
            "sp-runtime-interface" => {
                table.insert("version", GP_RUNTIME_INTERFACE_VERSION.into());
                table.insert("package", "gp-runtime-interface".into());
            }
            // Depends on sp-wasm-interface.
            //
            // ref:
            // - sp-runtime-interface-18.0.0
            // - sp-runtime-interface-proc-macro-12.0.0
            "sp-crypto-ec-utils" => {
                table.insert("package", "gp-crypto-ec-utils".into());
            }
            _ => {}
        }

        table.remove("branch");
        table.remove("git");
    }
}
