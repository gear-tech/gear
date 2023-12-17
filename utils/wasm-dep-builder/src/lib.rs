// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

mod builder;

use crate::builder::build_wasm;
use cargo_metadata::{DependencyKind, MetadataCommand};
use fs4::FileExt;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

const DEMO_OCCURRED: &str = "demo's script has been executed";
const BUILDER_OCCURRED: &str = "builder has built this demo";

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    wasm_dep_builder: Option<WasmDepBuilderMetadata>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WasmDepBuilderMetadata {
    Demo(DemoMetadata),
    Builder(BuilderMetadata),
}

impl WasmDepBuilderMetadata {
    fn into_demo(self) -> Option<DemoMetadata> {
        match self {
            WasmDepBuilderMetadata::Demo(demo) => Some(demo),
            WasmDepBuilderMetadata::Builder(_) => None,
        }
    }

    fn into_builder(self) -> Option<BuilderMetadata> {
        match self {
            WasmDepBuilderMetadata::Demo(_) => None,
            WasmDepBuilderMetadata::Builder(builder) => Some(builder),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DemoMetadata {
    exclude_features: BTreeSet<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuilderMetadata {
    exclude: BTreeSet<String>,
}

fn out_dir() -> PathBuf {
    env::var("OUT_DIR").unwrap().into()
}

fn profile() -> String {
    out_dir()
        .components()
        .rev()
        .take_while(|c| c.as_os_str() != "target")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take_while(|c| c.as_os_str() != "build")
        .last()
        .expect("Path should have subdirs in the `target` dir")
        .as_os_str()
        .to_string_lossy()
        .into()
}

fn wasm_projects_dir() -> PathBuf {
    let profile = profile();

    out_dir()
        .ancestors()
        .find(|path| path.ends_with(&profile))
        .and_then(|path| path.parent())
        .map(|p| p.to_owned())
        .expect("Could not find target directory")
        .join("wasm-projects")
}

fn wasm32_target_dir() -> PathBuf {
    wasm_projects_dir().join("wasm32-unknown-unknown")
}

fn lock_file(pkg_name: String) -> PathBuf {
    wasm32_target_dir()
        .join(profile())
        .join(format!("{}.lock", pkg_name))
}

pub fn builder() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir: PathBuf = manifest_dir.into();
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let out_dir = out_dir();
    let wasm32_target_dir = wasm32_target_dir().join(profile());

    fs::create_dir_all(&wasm32_target_dir).unwrap();

    // track if demo is being added or removed
    let cargo_toml = manifest_dir.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", cargo_toml.display());

    // track `OUT_DIR` because cargo can change it during development
    // and all of `wasm_binary.rs` files will be gone
    // TODO: possibly env tracking is not enough to generate `wasm_binary.rs` files
    println!("cargo:rerun-if-env-changed=OUT_DIR");

    // don't track features because they are resolved by `cargo metadata`
    // and always set in TOML files and not via CLI
    env::set_var("__GEAR_WASM_BUILDER_NO_FEATURES_TRACKING", "1");

    let metadata = MetadataCommand::new().no_deps().exec().unwrap();
    let package = metadata
        .packages
        .iter()
        .find(|package| package.name == pkg_name)
        .unwrap();

    let config = serde_json::from_value::<Option<PackageMetadata>>(package.metadata.clone())
        .unwrap()
        .unwrap_or_default()
        .wasm_dep_builder
        .map(|config| config.into_builder().expect("Builder config expected"))
        .unwrap_or_default();

    let mut wasm_binaries = String::new();
    let mut packages_to_build = BTreeMap::new();

    for dep in package
        .dependencies
        .iter()
        .filter(|dep| dep.kind == DependencyKind::Development)
        .filter(|dep| !config.exclude.contains(&dep.name))
        .filter(|dep| dep.name.starts_with("demo-"))
    {
        let pkg = metadata
            .packages
            .iter()
            .find(|pkg| pkg.name == dep.name)
            .unwrap();

        // check if demo has this crate as dependency
        let contains = pkg
            .dependencies
            .iter()
            .any(|dep| dep.name == env!("CARGO_PKG_NAME"));
        if !contains {
            println!(
                "cargo:warning=`{}` doesn't have `wasm-dep-builder` dependency, skipping",
                dep.name
            );
            continue;
        }

        let dep_name = dep.name.replace('-', "_");
        let pkg_metadata = serde_json::from_value::<Option<PackageMetadata>>(pkg.metadata.clone())
            .unwrap()
            .unwrap_or_default()
            .wasm_dep_builder
            .map(|config| config.into_demo().expect("Demo config expected"))
            .unwrap_or_default();

        wasm_binaries += &format!(
            r#"
pub mod {dep_name} {{
    include!(concat!(env!("OUT_DIR"), "/{dep_name}/wasm_binary.rs"));
}}
            "#,
        );

        let features: BTreeSet<String> = dep.features.iter().cloned().collect();
        let excluded_features = pkg_metadata.exclude_features;
        let features: BTreeSet<String> = features.difference(&excluded_features).cloned().collect();

        let lock = lock_file(dep_name);
        println!("cargo:rerun-if-changed={}", lock.display());
        println!("cargo:warning=tracking {}", lock.display());

        let lock_exists = lock.exists();
        let mut lock = fs::File::options()
            .create(true)
            .write(true)
            .read(true)
            .open(lock)
            .unwrap();
        lock.lock_exclusive().unwrap();

        let mut content = String::new();
        lock.read_to_string(&mut content).unwrap();

        println!(
            r#"cargo:warning=!lock_exists || content == "{DEMO_OCCURRED}" <=> {} || {}"#,
            !lock_exists,
            content == DEMO_OCCURRED
        );
        #[allow(clippy::overly_complex_bool_expr)]
        if true || !lock_exists || content == DEMO_OCCURRED {
            println!("cargo:warning=rebuilding...");

            packages_to_build.insert(pkg.name.clone(), features);

            lock.set_len(0).unwrap();
            lock.seek(SeekFrom::Start(0)).unwrap();
            write!(lock, "{BUILDER_OCCURRED}").unwrap();
        }
    }

    println!("cargo:warning={:?}", packages_to_build);
    if !packages_to_build.is_empty() {
        build_wasm(packages_to_build);
    }

    fs::write(out_dir.join("wasm_binaries.rs"), wasm_binaries).unwrap();
}

pub fn demo() {
    if env::var("__GEAR_WASM_BUILDER_NO_BUILD").is_ok() {
        // we entered `gear-wasm-builder`
        return;
    }

    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let pkg_name = pkg_name.replace('-', "_");
    let wasm32_target_dir = wasm32_target_dir().join(profile());

    fs::create_dir_all(wasm32_target_dir).unwrap();

    let lock = lock_file(pkg_name);
    println!("cargo:warning=[DEMO] {}", lock.display());
    let mut lock = fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(lock)
        .unwrap();
    lock.lock_exclusive().unwrap();

    write!(lock, "{DEMO_OCCURRED}").unwrap();
}
