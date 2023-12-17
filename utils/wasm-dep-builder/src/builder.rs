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

use crate::{out_dir, profile, wasm32_target_dir, wasm_projects_dir};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    process::Command,
};

pub fn build_wasm(packages: BTreeMap<String, BTreeSet<String>>) {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());

    let wasm32_target_dir = wasm32_target_dir().join(profile());

    let packages_and_features = packages
        .iter()
        .map(|(pkg, features)| {
            (
                pkg,
                features
                    .iter()
                    .map(|feature| format!("{}/{}", pkg, feature))
                    .collect::<Vec<_>>()
                    .join(","),
            )
        })
        .flat_map(|(pkg, features)| {
            vec![
                "--package".to_string(),
                pkg.clone(),
                "--features".to_string(),
                features,
            ]
        });

    let mut cargo = Command::new(cargo);
    cargo
        .arg("build")
        .arg("--no-default-features")
        .args(packages_and_features)
        .arg("--profile")
        .arg(profile().replace("debug", "dev"))
        .env("__GEAR_WASM_BUILDER_NO_BUILD", "1")
        .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
        .env("CARGO_TARGET_DIR", wasm_projects_dir());
    println!("cargo:warning={:?}", cargo);
    let status = cargo.status().expect("Failed to execute cargo command");
    assert!(status.success());

    for (pkg, _) in packages {
        let pkg = pkg.replace('-', "_");

        let out_dir = out_dir().join(&pkg);
        fs::create_dir_all(&out_dir).unwrap();

        // TODO: optimize binary
        fs::write(
            out_dir.join("wasm_binary.rs"),
            format!(
                r#"
    #[allow(unused)]
    pub const WASM_BINARY: &[u8] = include_bytes!("{}");
    #[allow(unused)]
    pub const WASM_BINARY_OPT: &[u8] = WASM_BINARY;
    "#,
                wasm32_target_dir.join(format!("{}.wasm", pkg)).display()
            ),
        )
        .unwrap();
    }
}
