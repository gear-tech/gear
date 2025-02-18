// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

/// The working version of sp-wasm-interface.
pub const GP_RUNTIME_INTERFACE_VERSION: &str = "18.0.0";

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
pub fn patch(pkg: &Package, is_published: bool, is_actualized: bool) -> Result<Manifest> {
    let mut manifest = Manifest::new(pkg, is_published, is_actualized)?;
    let doc = &mut manifest.mutable_manifest;

    match manifest.name.as_str() {
        "gear-core-processor" => core_processor::patch(doc),
        "gear-sandbox" => sandbox::patch(doc),
        "gear-sandbox-host" => sandbox_host::patch(doc),
        "gear-sandbox-interface" => sandbox_interface::patch(doc),
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

/// sandbox handler.
mod sandbox {
    use toml_edit::DocumentMut;

    /// Replace the wasmi module to the crates-io version.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wasmi) = manifest["dependencies"]["wasmi"].as_table_like_mut() else {
            return;
        };
        wasmi.insert("package", toml_edit::value("gwasmi"));
        wasmi.insert("version", toml_edit::value("0.30.0"));
        wasmi.remove("branch");
        wasmi.remove("git");
    }
}

/// sandbox interface handler
mod sandbox_interface {
    use super::GP_RUNTIME_INTERFACE_VERSION;
    use toml_edit::DocumentMut;

    /// Patch the manifest of runtime-interface.
    ///
    /// We need to patch the manifest of package again because
    /// `sp_runtime_interface_proc_macro` includes some hardcode
    /// that could not locate alias packages.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wi) = manifest["dependencies"]["sp-runtime-interface"].as_table_like_mut() else {
            return;
        };
        wi.insert("version", toml_edit::value(GP_RUNTIME_INTERFACE_VERSION));
        wi.insert("package", toml_edit::value("gp-runtime-interface"));
        wi.remove("workspace");
    }
}

/// sandbox_host handler.
mod sandbox_host {
    use toml_edit::DocumentMut;

    /// Replace the wasmi module to the crates-io version.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(wasmi) = manifest["dependencies"]["sandbox-wasmi"].as_table_like_mut() else {
            return;
        };
        wasmi.insert("package", toml_edit::value("wasmi"));
        wasmi.insert("version", toml_edit::value("0.13.2"));
        wasmi.remove("workspace");
    }
}

/// substrate handler.
mod substrate {
    use super::GP_RUNTIME_INTERFACE_VERSION;
    use toml_edit::InlineTable;

    /// Patch the substrate packages in the manifest of workspace.
    ///
    /// Substrate packages on crates-io currently have no version management
    /// (<https://github.com/paritytech/polkadot-sdk/issues/2809>),
    /// the following versions are pinned to frame-support-v22.0.0 on crates-io
    /// now, <https://crates.io/crates/frame-system/22.0.0/dependencies> for
    /// the details.
    ///
    /// NOTE: The packages inside of this function are located at
    /// <https://github.com/gear-tech/substrate/tree/cl/v1.1.x-crates-io>.
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
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
            _ => return,
        }

        table.remove("branch");
        table.remove("git");
    }
}
