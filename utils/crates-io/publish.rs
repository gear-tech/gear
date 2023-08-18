//! mini-program for publishing packages to crates.io.
use anyhow::Result;
use cargo_metadata::{DependencyKind, MetadataCommand};
use semver::VersionReq;
use std::collections::{BTreeMap, HashMap};

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
    let mut index = HashMap::<String, usize>::from_iter(
        PACKAGES.into_iter().enumerate().map(|(i, p)| (p.into(), i)),
    );
    let mut graph = BTreeMap::new();

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

    for p in graph.values() {
        println!("{:?}", p.name);
        // for package in packages {
        //     if package.name == "gmeta" {
        //         let t = toml::to_string_pretty(&package)?;
        //         println!("{}", t);
        //     }
        // }
    }

    Ok(())
}
