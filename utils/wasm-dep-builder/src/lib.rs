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
use serde::Deserialize;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    wasm_dep_builder: Config,
}

#[derive(Default, Deserialize)]
struct Config {
    exclude: BTreeSet<String>,
}

pub fn build_demos() {
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = PathBuf::from(out_dir);

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

        gear_wasm_builder::build();
    }

    fs::write(out_dir.join("wasm_binaries.rs"), wasm_binaries).unwrap();
}
