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
mod lock;

use crate::{
    builder::{BuildPackage, BuildPackages},
    lock::{BuilderLockFile, BuilderLockFileConfig, DemoLockFile, DemoLockFileConfig},
};
use cargo_metadata::MetadataCommand;
use globset::{Glob, GlobSet};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeSet, env, fmt, fs, path::PathBuf};

const NO_BUILD_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD";
const NO_BUILD_INNER_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD_INNER";

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
struct UnderscoreString(String);

impl UnderscoreString {
    pub fn original(&self) -> &String {
        &self.0
    }

    fn underscore(&self) -> String {
        self.0.replace('-', "_")
    }
}

impl<T: Into<String>> From<T> for UnderscoreString {
    fn from(s: T) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for UnderscoreString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.underscore(), f)
    }
}

impl fmt::Debug for UnderscoreString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl PartialEq for UnderscoreString {
    fn eq(&self, other: &Self) -> bool {
        self.underscore() == other.underscore()
    }
}

impl Eq for UnderscoreString {}

impl PartialOrd for UnderscoreString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnderscoreString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.underscore().cmp(&other.underscore())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    wasm_dep_builder: Option<WasmDepBuilderMetadata>,
}

#[derive(Deserialize, derive_more::Unwrap)]
#[serde(rename_all = "kebab-case")]
enum WasmDepBuilderMetadata {
    Demo(DemoMetadata),
    Builder(BuilderMetadata),
}

impl WasmDepBuilderMetadata {
    fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value::<Option<PackageMetadata>>(value)
            .unwrap()
            .and_then(|metadata| metadata.wasm_dep_builder)
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DemoMetadata {
    #[serde(default)]
    exclude_features: BTreeSet<String>,
}

impl DemoMetadata {
    fn from_value(value: serde_json::Value) -> Self {
        WasmDepBuilderMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_demo())
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuilderMetadata {
    #[serde(default = "BuilderMetadata::default_include")]
    include: GlobSet,
    #[serde(default)]
    exclude: BTreeSet<String>,
}

impl Default for BuilderMetadata {
    fn default() -> Self {
        Self {
            include: Self::default_include(),
            exclude: Default::default(),
        }
    }
}

impl BuilderMetadata {
    fn from_value(value: serde_json::Value) -> Self {
        WasmDepBuilderMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_builder())
            .unwrap_or_default()
    }

    fn default_include() -> GlobSet {
        GlobSet::builder()
            .add(Glob::new("demo-*").unwrap())
            .build()
            .unwrap()
    }

    fn filter_dep(&self, pkg_name: &str) -> bool {
        !self.exclude.contains(pkg_name) && self.include.is_match(pkg_name)
    }
}

fn manifest_dir() -> PathBuf {
    env::var("CARGO_MANIFEST_DIR").unwrap().into()
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

fn get_no_build_env() -> bool {
    env::var(NO_BUILD_ENV).is_ok()
}

fn get_no_build_inner_env() -> bool {
    env::var(NO_BUILD_INNER_ENV).is_ok()
}

pub fn builder() {
    println!("cargo:rerun-if-env-changed={NO_BUILD_ENV}");

    let manifest_dir = manifest_dir();
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let out_dir = out_dir();

    let wasm32_target_dir = wasm32_target_dir().join(profile());
    fs::create_dir_all(&wasm32_target_dir).unwrap();

    let build_rs = manifest_dir.join("build.rs");
    println!("cargo:rerun-if-changed={}", build_rs.display());
    // track if demo is being added or removed
    let cargo_toml = manifest_dir.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", cargo_toml.display());

    let metadata = MetadataCommand::new().no_deps().exec().unwrap();
    let pkg = metadata
        .packages
        .iter()
        .find(|package| package.name == pkg_name)
        .unwrap();

    let builder_metadata = BuilderMetadata::from_value(pkg.metadata.clone());

    let mut packages = BuildPackages::default();
    let mut locks = Vec::new();

    for dep in pkg
        .dependencies
        .iter()
        .filter(|dep| builder_metadata.filter_dep(&dep.name))
    {
        let pkg = metadata
            .packages
            .iter()
            .find(|pkg| pkg.name == dep.name)
            .unwrap();

        println!("cargo:rerun-if-changed={}", pkg.manifest_path);

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

        let demo_metadata = DemoMetadata::from_value(pkg.metadata.clone());

        let lock = lock::file_path(&dep.name);
        println!("cargo:rerun-if-changed={}", lock.display());
        let mut lock = BuilderLockFile::open(&dep.name);

        let lock_config = lock.read();
        let build_pkg = BuildPackage::new(pkg, lock_config, demo_metadata.exclude_features);

        let features = build_pkg.features();
        locks.push((
            lock,
            BuilderLockFileConfig {
                features: features.clone(),
            },
        ));

        packages.insert(build_pkg);
    }

    println!("cargo:warning={:?}", packages);
    packages.build();

    for (mut lock, config) in locks {
        lock.write(config);
    }

    let wasm_binaries = packages.wasm_binaries();
    fs::write(out_dir.join("wasm_binaries.rs"), wasm_binaries).unwrap();
}

pub fn demo() {
    if get_no_build_inner_env() {
        // we entered `wasm-dep-builder`
        return;
    }

    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let wasm32_target_dir = wasm32_target_dir().join(profile());

    fs::create_dir_all(wasm32_target_dir).unwrap();

    let features = env::vars()
        .filter_map(|(key, _val)| key.strip_prefix("CARGO_FEATURE_").map(str::to_lowercase))
        .map(UnderscoreString)
        .collect();
    let config = DemoLockFileConfig { features };

    let mut lock = DemoLockFile::open(pkg_name);
    lock.write(config);
}
