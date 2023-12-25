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
use cargo_metadata::MetadataCommand;
use fs4::FileExt;
use globset::GlobSet;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    env, fmt, fs,
    io::{Read, Seek, SeekFrom},
    marker::PhantomData,
    path::PathBuf,
};

const DEFAULT_EXCLUDED_FEATURES: [&str; 3] = ["default", "std", "wasm-wrapper"];

#[derive(derive_more::Display, Clone, Serialize, Deserialize)]
#[display(fmt = "{_0}")]
#[serde(transparent)]
struct UnderscoreString(String);

impl UnderscoreString {
    fn underscore(&self) -> String {
        self.0.replace('-', "_")
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
        self.underscore().partial_cmp(&other.underscore())
    }
}

impl Ord for UnderscoreString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.underscore().cmp(&other.underscore())
    }
}

#[derive(Default, Deserialize)]
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
            .unwrap_or_default()
            .wasm_dep_builder
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

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuilderMetadata {
    include: Option<GlobSet>,
    #[serde(default)]
    exclude: BTreeSet<String>,
}

impl BuilderMetadata {
    fn from_value(value: serde_json::Value) -> Self {
        WasmDepBuilderMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_builder())
            .unwrap_or_default()
    }

    fn excludes(&self, pkg_name: &str) -> bool {
        self.exclude.contains(pkg_name)
    }

    fn includes(&self, pkg_name: &str) -> bool {
        self.include
            .as_ref()
            .map(|set| set.is_match(pkg_name))
            .unwrap_or(false)
    }
}

#[derive(Debug, Serialize, Deserialize, derive_more::Unwrap)]
#[serde(rename_all = "kebab-case")]
enum LockFileConfig {
    Demo(DemoLockFileConfig),
    Builder(BuilderLockFileConfig),
}

#[derive(Debug, Serialize, Deserialize)]
struct DemoLockFileConfig {
    features: BTreeSet<UnderscoreString>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BuilderLockFileConfig {
    features: BTreeSet<String>,
}

struct DemoLockFile(());

struct BuilderLockFile(());

struct LockFile<T> {
    file: fs::File,
    _marker: PhantomData<T>,
}

impl LockFile<()> {
    fn path(pkg_name: impl AsRef<str>) -> PathBuf {
        let pkg_name = pkg_name.as_ref().replace('-', "_");
        wasm32_target_dir()
            .join(profile())
            .join(format!("{}.lock", pkg_name))
    }
}

impl LockFile<DemoLockFile> {
    fn open_for_demo(pkg_name: impl AsRef<str>) -> Self {
        let path = LockFile::path(pkg_name);
        println!("cargo:warning=[DEMO] {}", path.display());
        let file = fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        file.lock_exclusive().unwrap();

        Self {
            file,
            _marker: PhantomData,
        }
    }

    fn write(&mut self, config: DemoLockFileConfig) {
        serde_json::to_writer(&mut self.file, &LockFileConfig::Demo(config)).unwrap();
    }
}

impl LockFile<BuilderLockFile> {
    fn open_for_builder(path: PathBuf) -> Self {
        let file = fs::File::options()
            .create(true)
            .write(true)
            .read(true)
            .open(path)
            .unwrap();
        file.lock_exclusive().unwrap();

        Self {
            file,
            _marker: PhantomData,
        }
    }

    fn read(&mut self) -> Option<LockFileConfig> {
        let mut config = String::new();
        self.file.read_to_string(&mut config).unwrap();
        (!config.is_empty()).then(|| serde_json::from_str(&config).unwrap())
    }

    fn write(&mut self, config: BuilderLockFileConfig) {
        self.file.set_len(0).unwrap();
        self.file.seek(SeekFrom::Start(0)).unwrap();
        serde_json::to_writer(&mut self.file, &LockFileConfig::Builder(config)).unwrap();
    }
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

    let metadata = MetadataCommand::new().no_deps().exec().unwrap();
    let package = metadata
        .packages
        .iter()
        .find(|package| package.name == pkg_name)
        .unwrap();

    let builder_metadata = BuilderMetadata::from_value(package.metadata.clone());

    let mut packages = BTreeMap::new();

    for dep in package
        .dependencies
        .iter()
        .filter(|dep| !builder_metadata.excludes(&dep.name))
        .filter(|dep| builder_metadata.includes(&dep.name))
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

        let demo_metadata = DemoMetadata::from_value(pkg.metadata.clone());

        let lock = LockFile::path(&dep.name);
        println!("cargo:rerun-if-changed={}", lock.display());
        println!("cargo:warning=tracking {}", lock.display());
        let mut lock = LockFile::open_for_builder(lock);

        let config = lock.read();
        println!("cargo:warning={:?}", config);

        let features = match &config {
            Some(LockFileConfig::Demo(DemoLockFileConfig { features })) => {
                let excluded_features = demo_metadata
                    .exclude_features
                    .into_iter()
                    .map(UnderscoreString)
                    .chain(
                        DEFAULT_EXCLUDED_FEATURES
                            .map(str::to_string)
                            .map(UnderscoreString),
                    )
                    .collect();
                let features: BTreeSet<UnderscoreString> =
                    features.difference(&excluded_features).cloned().collect();

                let orig_features: BTreeSet<UnderscoreString> =
                    pkg.features.keys().cloned().map(UnderscoreString).collect();

                let features: BTreeSet<String> = orig_features
                    .intersection(&features)
                    .cloned()
                    .map(|s| s.0)
                    .collect();

                println!("cargo:warning=rebuilding...");

                lock.write(BuilderLockFileConfig {
                    features: features.clone(),
                });

                Some(features)
            }
            Some(LockFileConfig::Builder(BuilderLockFileConfig { features })) => {
                Some(features.clone())
            }
            None => None,
        };
        packages.insert(pkg.name.clone(), features);
    }

    println!("cargo:warning={:?}", packages);
    let wasm_binaries = build_wasm(packages);
    fs::write(out_dir.join("wasm_binaries.rs"), wasm_binaries).unwrap();
}

pub fn demo() {
    if env::var("__GEAR_WASM_BUILDER_NO_BUILD").is_ok() {
        // we entered `gear-wasm-builder`
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

    let mut lock = LockFile::open_for_demo(pkg_name);
    lock.write(config);
}
