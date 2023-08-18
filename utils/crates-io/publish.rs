//! mini-program for publishing packages to crates.io.
use anyhow::Result;
use cargo_metadata::{DependencyKind, MetadataCommand};
use semver::VersionReq;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
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

fn main() -> Result<()> {
    let metadata = MetadataCommand::new().no_deps().exec()?;
    let mut graph = BTreeMap::new();
    let index = HashMap::<String, usize>::from_iter(
        PACKAGES.into_iter().enumerate().map(|(i, p)| (p.into(), i)),
    );

    for mut p in metadata.packages.into_iter() {
        if !index.contains_key(&p.name) {
            continue;
        }

        let version = VersionReq::parse(&p.version.to_string())?;
        for d in p.dependencies.iter_mut() {
            if d.kind != DependencyKind::Normal {
                continue;
            }

            if index.contains_key(&d.name) {
                d.req = version.clone();
            }
        }

        graph.insert(index.get(&p.name), p);
    }

    for package in graph.values() {
        let manifest = package.manifest_path.as_str();
        fs::write(manifest, toml::to_string_pretty(package)?)?;

        let status = publish(manifest)?;
        if !status.success() {
            // The most likely reason for failure is that
            // we have reached the rate limit of crates.io.
            //
            // Need to wait for 10 mins and try again. here
            // we use 11 mins to be safe.
            //
            // Only retry for once, if it still fails, we
            // will just give up.
            thread::sleep(Duration::from_secs(660));
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
        .status()
        .map_err(Into::into)
}
