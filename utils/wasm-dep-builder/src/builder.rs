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

use crate::{profile, wasm32_target_dir, wasm_projects_dir};
use gear_wasm_builder::{
    optimize,
    optimize::{OptType, Optimizer},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    process::Command,
};

pub fn build_wasm(packages: BTreeMap<String, Option<BTreeSet<String>>>) -> String {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let wasm32_target_dir = wasm32_target_dir().join(profile());

    let packages_and_features = packages
        .iter()
        .map(|(pkg, features)| {
            (
                pkg,
                features
                    .iter()
                    .flatten()
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
        })
        .collect::<Vec<String>>();

    if !packages_and_features.is_empty() {
        let mut cargo = Command::new(cargo);
        cargo
            .arg("build")
            .arg("--no-default-features")
            .args(packages_and_features)
            .arg("--profile")
            .arg(profile().replace("debug", "dev"))
            .env("__GEAR_WASM_BUILDER_NO_BUILD", "1")
            .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
            .env("CARGO_TARGET_DIR", wasm_projects_dir())
            // remove host flags
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        println!("cargo:warning={:?}", cargo);
        let status = cargo.status().expect("Failed to execute cargo command");
        assert!(status.success());
    }

    let mut wasm_binaries = String::new();

    for (pkg, build_required) in packages {
        let pkg = pkg.replace('-', "_");

        let wasm = wasm32_target_dir.join(format!("{}.wasm", pkg));
        let mut wasm_opt = wasm.clone();
        wasm_opt.set_extension("opt.wasm");

        wasm_binaries += &format!(
            r#"
pub mod {pkg} {{
    pub use ::{pkg}::*;
    
    pub const WASM_BINARY_BLOATY: &[u8] = include_bytes!("{}");
    pub const WASM_BINARY: &[u8] = include_bytes!("{}");
}}
                    "#,
            wasm.display(),
            wasm_opt.display()
        );

        if build_required.is_none() {
            continue;
        }

        optimize::optimize_wasm(wasm.clone(), wasm_opt.clone(), "4", true).unwrap();

        let mut optimizer = Optimizer::new(wasm_opt.clone()).unwrap();
        optimizer.insert_stack_end_export().unwrap_or_else(|err| {
            println!("cargo:warning=Cannot insert stack end export into `{pkg}`: {err}")
        });
        optimizer.strip_custom_sections();

        let binary_opt = optimizer.optimize(OptType::Opt).unwrap();
        fs::write(&wasm_opt, binary_opt).unwrap();
    }

    wasm_binaries
}
