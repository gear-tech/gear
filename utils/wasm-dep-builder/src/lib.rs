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
    lock::{BinariesLockFile, BinariesLockFileConfig, ProgramLockFile, ProgramLockFileConfig},
    metadata::{BinariesMetadata, ProgramMetadata},
    utils::{
        get_no_build_env, get_no_build_inner_env, manifest_dir, out_dir, profile,
        wasm32_target_dir, UnderscoreString,
    },
};
use anyhow::Context;
use cargo_metadata::{camino::Utf8PathBuf, Metadata, MetadataCommand, Package};
use std::{
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

const NO_BUILD_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD";

/// [`track_program()`] must write config file anyway
/// except case where builder compiles program to WASM so
/// we don't want creation of garbage config files
/// because [`track_program()`] will be called again but in another environment
const NO_BUILD_INNER_ENV: &str = "__GEAR_WASM_BUILDER_NO_BUILD_INNER";

const NO_PATH_REMAP_ENV: &str = "__GEAR_WASM_BUILDER_NO_PATH_REMAP";

struct PostPackage {
    name: UnderscoreString,
    manifest_path: Utf8PathBuf,
    wasm_bloaty: PathBuf,
    wasm: PathBuf,
}

impl PostPackage {
    fn new(pkg: &Package, build_pkg: &BuildPackage) -> Self {
        Self {
            name: build_pkg.name().clone(),
            manifest_path: pkg.manifest_path.clone(),
            wasm_bloaty: build_pkg.wasm_bloaty_path().to_path_buf(),
            wasm: build_pkg.wasm_path().to_path_buf(),
        }
    }

    fn write_binpath(&self) {
        let path = self
            .manifest_path
            .parent()
            .expect("file path must have parent")
            .join(".binpath");
        let contents = self.wasm_bloaty.with_extension("").display().to_string();
        fs::write(&path, contents)
            .with_context(|| format!("failed to write `.binpath` at {path}"))
            .unwrap();
    }

    fn write_wasm_binary(&self, wasm_binaries: &mut String) {
        let pkg_name = &self.name;
        let (wasm_bloaty, wasm) = if get_no_build_env() {
            ("&[]".to_string(), "&[]".to_string())
        } else {
            (
                format!(r#"include_bytes!("{}")"#, to_unix_path(&self.wasm_bloaty)),
                format!(r#"include_bytes!("{}")"#, to_unix_path(&self.wasm)),
            )
        };

        let _ = write!(
            wasm_binaries,
            r#"
pub mod {pkg_name} {{
    pub use ::{pkg_name}::*;
    
    pub const WASM_BINARY_BLOATY: &[u8] = {wasm_bloaty};
    pub const WASM_BINARY: &[u8] = {wasm};
}}
                    "#,
        );
    }
}

fn to_unix_path(path: &Path) -> String {
    // Windows uses `\\` path delimiter which cannot be used in `include_*` Rust macros
    path.display().to_string().replace('\\', "/")
}

fn find_pkg<'a>(metadata: &'a Metadata, pkg_name: &str) -> &'a Package {
    metadata
        .packages
        .iter()
        .find(|package| package.name == pkg_name)
        .unwrap()
}

/// Build Gear programs to WASM binaries.
///
/// Collects every program by listing crate dependencies and
/// tracks changes via program lock file.
pub fn build_binaries() {
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

    let binaries_metadata = BinariesMetadata::from_value(pkg.metadata.clone());

    let mut build_packages =
        BuildPackages::new(metadata.workspace_root.clone().into_std_path_buf());
    let mut post_actions = Vec::new();

    for dep in pkg
        .dependencies
        .iter()
        .filter(|dep| binaries_metadata.filter_dep(&dep.name))
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

        let program_metadata = ProgramMetadata::from_value(pkg.metadata.clone());

        let lock = lock::file_path(&dep.name);
        println!("cargo:rerun-if-changed={}", lock.display());
        let mut lock = BinariesLockFile::open(&dep.name);

        let lock_config = lock.read_any();
        let build_pkg = BuildPackage::new(pkg, lock_config, program_metadata.exclude_features);

        let features = build_pkg.features().clone();
        post_actions.push((
            lock,
            BinariesLockFileConfig { features },
            PostPackage::new(pkg, &build_pkg),
        ));

        build_packages.insert(build_pkg);
    }

    println!("cargo:warning={:?}", build_packages);
    let packages_built = build_packages.build();

    let mut wasm_binaries = String::new();
    for (mut lock, config, package) in post_actions {
        // we don't need to write config in lock file
        // because we didn't build anything so next time when
        // `__GEAR_WASM_BUILDER_NO_BUILD` is changed builder will mark
        // crate as dirty and do an actual build
        if packages_built {
            lock.write(config);
        }

        package.write_binpath();
        package.write_wasm_binary(&mut wasm_binaries);
    }

    fs::write(out_dir.join("wasm_binaries.rs"), wasm_binaries).unwrap();
}

/// Track Gear program to build.
///
/// Never calls any `cargo:rerun-if` instructions to
/// keep default cargo build script invocation heuristics such as
/// tracking of every project file, its dependencies and features.
///
/// On every build script invocation just writes config to lock file.
pub fn track_program() {
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
    let config = ProgramLockFileConfig { features };

    let mut lock = ProgramLockFile::open(pkg_name);
    lock.write(config);
}
