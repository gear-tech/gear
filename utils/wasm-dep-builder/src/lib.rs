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
mod metadata;
mod utils;

use crate::{
    builder::{BuildPackage, BuildPackages},
    lock::{BuilderLockFile, BuilderLockFileConfig, DemoLockFile, DemoLockFileConfig},
    metadata::{BuilderMetadata, DemoMetadata},
    utils::{
        get_no_build_inner_env, manifest_dir, out_dir, profile, wasm32_target_dir, UnderscoreString,
    },
};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use std::{env, fs};

const NO_BUILD_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD";
const NO_BUILD_INNER_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD_INNER";
const NO_PATH_REMAP_ENV: &str = "__GEAR_WASM_BUILDER_NO_PATH_REMAP";

fn find_pkg<'a>(metadata: &'a Metadata, pkg_name: &str) -> &'a Package {
    metadata
        .packages
        .iter()
        .find(|package| package.name == pkg_name)
        .unwrap()
}

pub fn builder() {
    println!("cargo:rerun-if-env-changed={NO_BUILD_ENV}");
    println!("cargo:rerun-if-env-changed={NO_PATH_REMAP_ENV}");

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
    let pkg = find_pkg(&metadata, &pkg_name);

    let builder_metadata = BuilderMetadata::from_value(pkg.metadata.clone());

    let mut packages = BuildPackages::new(metadata.workspace_root.clone().into_std_path_buf());
    let mut locks = Vec::new();

    for dep in pkg
        .dependencies
        .iter()
        .filter(|dep| builder_metadata.filter_dep(&dep.name))
    {
        let pkg = find_pkg(&metadata, &dep.name);

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

    if packages.skip_build() {
        for (mut lock, config) in locks {
            lock.write(config);
        }
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
