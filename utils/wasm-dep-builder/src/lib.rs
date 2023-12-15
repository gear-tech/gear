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

use cargo_metadata::{DependencyKind, MetadataCommand};
use fs4::FileExt;
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

pub const DEMO_OCCURRED: &str = "demo's script has been executed";
pub const BUILDER_OCCURRED: &str = "builder has built this demo";

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    wasm_dep_builder: Config,
}

#[derive(Default, Deserialize)]
struct Config {
    exclude: BTreeSet<String>,
}

fn out_dir() -> PathBuf {
    env::var("OUT_DIR").unwrap().into()
}

fn wasm32_target_dir() -> PathBuf {
    let out_dir = out_dir();

    let profile: String = out_dir
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
        .into();

    let target_dir = out_dir
        .ancestors()
        .find(|path| path.ends_with(&profile))
        .and_then(|path| path.parent())
        .map(|p| p.to_owned())
        .expect("Could not find target directory");

    target_dir
        .join("wasm-projects")
        .join(&profile)
        .join("wasm32-unknown-unknown")
        .join(&profile)
}

pub fn builder() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir: PathBuf = manifest_dir.into();
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let out_dir = out_dir();
    let wasm32_target_dir = wasm32_target_dir();

    fs::create_dir_all(&wasm32_target_dir).unwrap();

    let cargo_toml = manifest_dir.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", cargo_toml.display());

    let metadata = MetadataCommand::new().no_deps().exec().unwrap();
    let package = metadata
        .packages
        .into_iter()
        .find(|package| package.name == pkg_name)
        .unwrap();

    let pkg_metadata = serde_json::from_value::<Option<PackageMetadata>>(package.metadata)
        .unwrap()
        .unwrap_or_default();
    let config = pkg_metadata.wasm_dep_builder;

    let mut wasm_binaries = String::new();

    for dep in package
        .dependencies
        .into_iter()
        .filter(|dep| dep.kind == DependencyKind::Development)
        .filter(|dep| !config.exclude.contains(&dep.name))
        .filter(|dep| dep.name.starts_with("demo-"))
    {
        let dep_name = dep.name.replace('-', "_");

        let wasm_out_dir = out_dir.join(&dep_name);
        fs::create_dir_all(&wasm_out_dir).unwrap();
        env::set_var("OUT_DIR", wasm_out_dir);

        wasm_binaries += &format!(
            r#"
pub mod {dep_name} {{
    include!(concat!(env!("OUT_DIR"), "/{dep_name}/wasm_binary.rs"));
}}
            "#,
        );

        env::vars()
            .filter(|(key, _)| key.starts_with("CARGO_FEATURE_"))
            .for_each(|(key, _)| env::remove_var(key));

        for feature in dep.features {
            let key = format!("CARGO_FEATURE_{}", feature.to_uppercase());
            env::set_var(key, "1")
        }

        let path = dep.path.expect("Rust version >= 1.51 expected");
        env::set_var("CARGO_MANIFEST_DIR", path);

        let lock = wasm32_target_dir.join(format!("{}.lock", dep_name));
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
        if !lock_exists || content == DEMO_OCCURRED {
            println!("cargo:warning=rebuilding...");

            gear_wasm_builder::build();

            lock.set_len(0).unwrap();
            lock.seek(SeekFrom::Start(0)).unwrap();
            write!(lock, "{BUILDER_OCCURRED}").unwrap();
        }
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
    let wasm32_target_dir = wasm32_target_dir();

    fs::create_dir_all(&wasm32_target_dir).unwrap();

    let lock = wasm32_target_dir.join(format!("{}.lock", pkg_name));
    println!("cargo:warning=writing to {}", lock.display());
    let mut lock = fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(lock)
        .unwrap();
    lock.lock_exclusive().unwrap();

    write!(lock, "{DEMO_OCCURRED}").unwrap();
}
