//! Renaming handler

use crate::PATCHED_PACKAGES;
use anyhow::Result;
use cargo_metadata::Package;
use cargo_toml::{Dependency, Manifest};

/// Rename a package
pub fn package(pkg: &Package, manifest: &mut Manifest) -> Result<()> {
    // NOTE: This is a bug inside of crate cargo_toml, it should
    // not append crate-type = ["rlib"] to proc-macro crates, fixing
    // it by hacking it now.
    if pkg.name.ends_with("-codegen") {
        if let Some(product) = manifest.lib.as_mut() {
            product.crate_type = vec![];
        }
    }

    Ok(())
}

/// Rename a dependencies
pub fn deps(map: &mut Manifest, index: Vec<&String>, version: String) -> Result<()> {
    for (name, dep) in map.dependencies.iter_mut() {
        // No need to update dependencies for packages without
        // local dependencies.
        if !index.contains(&name) && !PATCHED_PACKAGES.contains(&name.as_str()) {
            continue;
        }

        let mut detail = if let Dependency::Detailed(detail) = &dep {
            detail.clone()
        } else {
            continue;
        };

        match name.as_ref() {
            // NOTE: the required version of sp-arithmetic is 6.0.0 in
            // git repo, but 7.0.0 in crates.io, so we need to fix it.
            "sp-arithmetic" => {
                detail.version = Some("7.0.0".into());
                detail.branch = None;
                detail.git = None;
            }
            "parity-wasm" => {
                detail.package = Some("gear-wasm".into());
            }
            _ => detail.version = Some(version.clone()),
        }

        *dep = Dependency::Detailed(detail);
    }

    Ok(())
}
