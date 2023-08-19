//! mini-program for publishing packages to crates.io.
use anyhow::Result;
use cargo_metadata::MetadataCommand;
use cargo_toml::{Dependency, Manifest, Value};
use crates_io::Registry;
use curl::easy::Easy;
use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    path::PathBuf,
    process::{Command, ExitStatus},
    thread,
    time::Duration,
};

/// Packages need to be published.
const PACKAGES: [&str; 16] = [
    // Packages without local dependencies.
    "gear-backend-codegen",
    "gear-common-codegen",
    "gear-core-errors",
    "gear-wasm-instrument",
    "gmeta-codegen",
    "gsdk-codegen",
    "gsys",
    // The packages below have local dependencies,
    // and should be published in order.
    "gmeta",
    "gear-core",
    "gear-core-processor",
    "gear-backend-common",
    "gear-backend-wasmi",
    "gear-common",
    "gsdk",
    "gcli",
    "gclient",
];

struct CratesIo {
    registry: Registry,
}

impl CratesIo {
    /// Create a new instance of `CratesIo`.
    pub fn new() -> Result<Self> {
        let mut handle = Easy::new();
        handle.useragent("gear-crates-io-manager")?;

        Ok(Self {
            registry: Registry::new_handle("https://crates.io".into(), None, handle, false),
        })
    }

    /// Verify if the package is published to crates.io.
    pub fn verify(&mut self, package: &str, version: &str) -> Result<bool> {
        // Here using limit = 1 since we are searching explicit
        // packages here.
        let (crates, _total) = self.registry.search(package, 1)?;
        if crates.len() != 1 {
            return Ok(false);
        }

        Ok(crates[0].max_version == version)
    }
}

fn main() -> Result<()> {
    let mut validator = CratesIo::new()?;
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
        if validator.verify(&p.name, &version)? {
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
            if !index.contains_key(name) {
                continue;
            }

            if let Dependency::Detailed(detail) = &dep {
                let mut detail = detail.clone();
                detail.version = Some(version.to_string());
                *dep = Dependency::Detailed(detail.clone());
            }
        }

        graph.insert(index.get(&p.name), (path, manifest));
    }

    for (path, manifest) in graph.values() {
        println!("Publishing {:?}", path);
        fs::write(path, toml::to_string_pretty(manifest)?)?;

        let path = path.to_string_lossy();
        let status = publish(&path)?;
        if !status.success() {
            println!(
                "Failed to publish package {}...\nRetry after 11 mins...",
                &path
            );
            // The most likely reason for failure is that
            // we have reached the rate limit of crates.io.
            //
            // Need to wait for 10 mins and try again. here
            // we use 11 mins to be safe.
            //
            // Only retry for once, if it still fails, we
            // will just give up.
            thread::sleep(Duration::from_secs(660));
            publish(&path)?;
        }
    }

    Ok(())
}

fn publish(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .arg("publish")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--allow-dirty")
        .arg("--dry-run")
        .status()
        .map_err(Into::into)
}
