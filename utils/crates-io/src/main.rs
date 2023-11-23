//! mini-program for publishing packages to crates.io.

use anyhow::Result;
use cargo_metadata::MetadataCommand;
use cargo_toml::{Dependency, Manifest, Value};
use crates_io_manager::{self as validator, PACKAGES, PATCHED_PACKAGES};
use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    path::PathBuf,
};

fn main() -> Result<()> {
    let metadata = MetadataCommand::new().no_deps().exec()?;
    let mut graph = BTreeMap::new();
    let index = HashMap::<String, usize>::from_iter(
        PACKAGES.into_iter().enumerate().map(|(i, p)| (p.into(), i)),
    );

    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../Cargo.toml")
        .canonicalize()?;
    let workspace = Manifest::from_path(&workspace_path)?;

    for p in metadata.packages.into_iter() {
        if !index.contains_key(&p.name) {
            continue;
        }

        let version = p.version.to_string();
        if validator::verify(&p.name, &version)? {
            println!("Package {}@{} already published.", &p.name, &version);
            continue;
        }

        let path = p.manifest_path.into_std_path_buf();
        let mut manifest = Manifest::<Value>::from_slice_with_metadata(&fs::read(&path)?)?;
        manifest.complete_from_path_and_workspace(&path, Some((&workspace, &workspace_path)))?;

        // NOTE: This is a bug inside of crate cargo_toml, it should
        // not append crate-type = ["rlib"] to proc-macro crates, fixing
        // it by hacking it now.
        if p.name.ends_with("-codegen") {
            if let Some(mut product) = manifest.lib {
                product.crate_type = vec![];
                manifest.lib = Some(product);
            }
        }

        for (name, dep) in manifest.dependencies.iter_mut() {
            // No need to update dependencies for packages without
            // local dependencies.
            if !index.contains_key(name) && !PATCHED_PACKAGES.contains(&name.as_str()) {
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
                _ => detail.version = Some(version.to_string()),
            }

            *dep = Dependency::Detailed(detail);
        }

        graph.insert(index.get(&p.name), (path, manifest));
    }

    for (path, manifest) in graph.values() {
        println!("Publishing {:?}", path);
        fs::write(path, toml::to_string_pretty(manifest)?)?;

        let path = path.to_string_lossy();
        let status = validator::publish(&path)?;
        if !status.success() {
            panic!("Failed to publish package {path}...");
        }
    }

    Ok(())
}
