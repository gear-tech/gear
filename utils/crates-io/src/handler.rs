// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Handlers for patching manifests.

/// The working version of sp-wasm-interface.
pub const GP_RUNTIME_INTERFACE_VERSION: &str = "18.0.0";

/// Get the crates-io name of the provided package.
pub fn crates_io_name(pkg: &str) -> &str {
    match pkg {
        "sp-allocator" => "gsp-allocator",
        "sp-wasm-interface" => "gsp-wasm-interface",
        "sp-wasm-interface-common" => "gsp-wasm-interface-common",
        "sc-executor" => "gsc-executor",
        "sc-executor-common" => "gsc-executor-common",
        "sc-executor-polkavm" => "gsc-executor-polkavm",
        "sc-executor-wasmtime" => "gsc-executor-wasmtime",
        "substrate-wasm-builder" => "gsubstrate-wasm-builder",
        _ => pkg,
    }
}

/// Apply publish-only manifest patches.
pub fn patch_publish(name: &str, doc: &mut toml_edit::DocumentMut) {
    match name {
        local_name if crate::GEAR_SUBSTRATE_DEPENDENCIES.contains(&local_name) => {
            substrate_fork::patch_manifest(local_name, doc)
        }
        "gear-sandbox" => sandbox::patch(doc),
        "gear-sandbox-interface" => sandbox_interface::patch(doc),
        _ => {}
    }
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
        local_name if crate::GEAR_SUBSTRATE_DEPENDENCIES.contains(&local_name) => {
            substrate_fork::patch_workspace(local_name, table)
        }
        sub if [
            "sc-",
            "sp-",
            "frame-",
            "try-runtime-cli",
            "binary-merkle-tree",
        ]
        .iter()
        .any(|p| sub.starts_with(p)) =>
        {
            substrate::patch_workspace(name, table)
        }
        _ => {}
    }
}

/// Patch the workspace manifest for publish-only state.
pub fn patch_publish_workspace(doc: &mut toml_edit::DocumentMut) {
    substrate_fork::patch_publish_workspace(doc);
}

/// Gear-maintained Polkadot SDK-compatible local crates.
mod substrate_fork {
    use toml_edit::{DocumentMut, InlineTable};

    /// Rename the package manifest to the Gear-owned crates.io alias.
    pub fn patch_manifest(local_name: &str, manifest: &mut DocumentMut) {
        let crates_io_name = super::crates_io_name(local_name);

        manifest["package"]["name"] = toml_edit::value(crates_io_name);
        manifest["package"]["documentation"] =
            toml_edit::value(format!("https://docs.rs/{crates_io_name}"));

        if local_name == "sc-executor-wasmtime" {
            // `sc-runtime-test` is a Polkadot SDK git-only dev dependency and
            // is not part of the Gear crates.io publish set.
            if let Some(dev_deps) = manifest["dev-dependencies"].as_table_like_mut() {
                dev_deps.remove("sc-runtime-test");
            }
        }

        if local_name == "substrate-wasm-builder" {
            super::substrate_wasm_builder::patch(manifest);
        }
    }

    /// Point the workspace dependency to the Gear-owned crates.io alias.
    pub fn patch_workspace(local_name: &str, table: &mut InlineTable) {
        table.insert("package", super::crates_io_name(local_name).into());

        table.remove("branch");
        table.remove("git");
        table.remove("rev");
    }

    /// Remove local Polkadot SDK source patches after copied crates are renamed.
    pub fn patch_publish_workspace(manifest: &mut DocumentMut) {
        let Some(patches) = manifest["patch"].as_table_like_mut() else {
            return;
        };

        let source = "https://github.com/paritytech/polkadot-sdk.git";
        let Some(polkadot_sdk) = patches
            .get_mut(source)
            .and_then(toml_edit::Item::as_table_mut)
        else {
            return;
        };

        for local_name in crate::GEAR_SUBSTRATE_DEPENDENCIES {
            polkadot_sdk.remove(local_name);
        }

        if polkadot_sdk.is_empty() {
            patches.remove(source);
        }
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

/// sandbox interface handler.
mod sandbox_interface {
    use super::GP_RUNTIME_INTERFACE_VERSION;
    use toml_edit::DocumentMut;

    /// Patch the manifest of runtime-interface.
    ///
    /// We need to patch the manifest of package again because
    /// `sp_runtime_interface_proc_macro` includes some hardcode
    /// that could not locate alias packages.
    pub fn patch(manifest: &mut DocumentMut) {
        if let Some(deps) = manifest["dependencies"].as_table_like_mut() {
            deps.remove("sc-executor");
        }

        if let Some(features) = manifest["features"].as_table_like_mut() {
            if let Some(default_features) = features
                .get_mut("default")
                .and_then(toml_edit::Item::as_array_mut)
            {
                default_features.retain(|feature| feature.as_str() != Some("host-api"));
            }

            if let Some(host_api_features) = features
                .get_mut("host-api")
                .and_then(toml_edit::Item::as_array_mut)
            {
                host_api_features.retain(|feature| feature.as_str() != Some("sc-executor"));
            }
        }

        let Some(wi) = manifest["dependencies"]["sp-runtime-interface"].as_table_like_mut() else {
            return;
        };
        wi.insert("version", toml_edit::value(GP_RUNTIME_INTERFACE_VERSION));
        wi.insert("package", toml_edit::value("gp-runtime-interface"));
        wi.remove("workspace");

        let Some(wi) = manifest["dependencies"]["sp-wasm-interface"].as_table_like_mut() else {
            return;
        };
        // The copied stable2409 executor crates use upstream `sp-wasm-interface`
        // 21.0.1, but `gear-sandbox-interface` still pairs with the old
        // Gear-published runtime-interface stack.
        wi.insert("version", toml_edit::value("15.0.0"));
        wi.insert("package", toml_edit::value("gp-wasm-interface"));
        wi.remove("workspace");
    }
}

/// substrate-wasm-builder handler.
mod substrate_wasm_builder {
    use toml_edit::DocumentMut;

    /// Keep the optional `metadata-hash` feature on the upstream executor
    /// stack. Gear only publishes the lower executor crates that are needed by
    /// its crates.io packages.
    pub fn patch(manifest: &mut DocumentMut) {
        let Some(sc_executor) = manifest["dependencies"]["sc-executor"].as_table_like_mut() else {
            return;
        };

        sc_executor.insert("version", toml_edit::value("0.40.1"));
        sc_executor.remove("workspace");
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
    /// NOTE: The `gp-*` packages below were published from
    /// <https://github.com/gear-tech/substrate/tree/cl/v1.1.x-crates-io>.
    // TODO: https://github.com/gear-tech/gear/issues/5485
    pub fn patch_workspace(name: &str, table: &mut InlineTable) {
        match name {
            // stable2409 executor crates require the upstream 21.0.1 API.
            "sp-wasm-interface" => {
                table.insert("version", "21.0.1".into());
                table.remove("package");
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
            // Related to ethexe-common.
            // We need to use newer version of binary-merkle-tree because we backport some features
            "binary-merkle-tree" => {
                table.insert("version", "16.1.1".into());
            }
            _ => return,
        }

        table.remove("path");
        table.remove("branch");
        table.remove("git");
        table.remove("rev");
    }
}
