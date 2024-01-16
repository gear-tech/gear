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

use crate::{
    lock::{BuilderLockFileConfig, DemoLockFileConfig, LockFileConfig},
    utils::{
        cargo_home_dir, get_no_build_env, get_no_map_remap_env, profile, wasm32_target_dir,
        wasm_projects_dir,
    },
    UnderscoreString, NO_BUILD_INNER_ENV,
};
use anyhow::Context;
use cargo_metadata::Package;
use gear_wasm_builder::{
    optimize,
    optimize::{OptType, Optimizer},
};
use std::{collections::BTreeSet, env, fmt::Write, fs, path::PathBuf, process::Command};

const DEFAULT_EXCLUDED_FEATURES: [&str; 3] = ["default", "std", "wasm-wrapper"];

#[derive(Debug, Eq, PartialEq)]
enum RebuildKind {
    Fresh,
    Dirty,
}

#[derive(Debug)]
pub struct BuildPackage {
    name: UnderscoreString,
    rebuild_kind: RebuildKind,
    features: BTreeSet<String>,
}

impl BuildPackage {
    pub fn new(pkg: &Package, config: LockFileConfig, excluded_features: BTreeSet<String>) -> Self {
        let name = UnderscoreString(pkg.name.clone());
        let (rebuild_kind, features) = Self::resolve_features(pkg, config, excluded_features);

        Self {
            name,
            rebuild_kind,
            features,
        }
    }

    fn resolve_features(
        pkg: &Package,
        config: LockFileConfig,
        excluded_features: BTreeSet<String>,
    ) -> (RebuildKind, BTreeSet<String>) {
        match config {
            LockFileConfig::Demo(DemoLockFileConfig { features }) => {
                let excluded_features = excluded_features
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

                (RebuildKind::Dirty, features)
            }
            LockFileConfig::Builder(BuilderLockFileConfig { features }) => {
                (RebuildKind::Fresh, features)
            }
        }
    }

    pub fn features(&self) -> &BTreeSet<String> {
        &self.features
    }

    fn wasm_paths(&self) -> (PathBuf, PathBuf) {
        let wasm32_target_dir = wasm32_target_dir().join(profile());
        let wasm = wasm32_target_dir.join(format!("{}.wasm", self.name));
        let mut wasm_opt = wasm.clone();
        wasm_opt.set_extension("opt.wasm");
        (wasm, wasm_opt)
    }

    fn to_unix_path(path: PathBuf) -> String {
        // Windows uses `\\` path delimiter which cannot be used in `include_*` Rust macros
        path.display().to_string().replace('\\', "/")
    }

    fn cargo_args(&self) -> impl Iterator<Item = String> {
        let pkg_name = self.name.original().clone();
        let features = self
            .features
            .iter()
            .map(|feature| format!("{pkg_name}/{feature}"))
            .collect::<Vec<_>>()
            .join(",");

        [
            "--package".to_string(),
            pkg_name,
            "--features".to_string(),
            features,
        ]
        .into_iter()
    }

    fn optimize(&self) {
        let (wasm, wasm_opt) = self.wasm_paths();

        optimize::optimize_wasm(wasm.clone(), wasm_opt.clone(), "4", true).unwrap();

        let mut optimizer = Optimizer::new(wasm_opt.clone()).unwrap();
        optimizer.insert_stack_end_export().unwrap_or_else(|err| {
            println!(
                "cargo:warning=Cannot insert stack end export into `{name}`: {err}",
                name = self.name.original()
            )
        });
        optimizer.strip_custom_sections();

        let binary_opt = optimizer.optimize(OptType::Opt).unwrap();
        fs::write(&wasm_opt, binary_opt).unwrap();
    }

    fn write_rust_mod(&self, output: &mut String) {
        let pkg_name = &self.name;
        let (wasm_bloaty, wasm) = if get_no_build_env() {
            ("&[]".to_string(), "&[]".to_string())
        } else {
            let (wasm_bloaty, wasm) = self.wasm_paths();
            (
                format!(r#"include_bytes!("{}")"#, Self::to_unix_path(wasm_bloaty)),
                format!(r#"include_bytes!("{}")"#, Self::to_unix_path(wasm)),
            )
        };

        let _ = write!(
            output,
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

#[derive(Debug)]
pub struct BuildPackages {
    packages: Vec<BuildPackage>,
    workspace_root: PathBuf,
}

impl BuildPackages {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            packages: Default::default(),
            workspace_root,
        }
    }

    pub fn insert(&mut self, build_pkg: BuildPackage) {
        self.packages.push(build_pkg);
    }

    fn rebuild_required(&self) -> bool {
        self.packages
            .iter()
            .any(|pkg| pkg.rebuild_kind == RebuildKind::Dirty)
    }

    fn cargo_args(&self) -> impl Iterator<Item = String> + '_ {
        self.packages.iter().flat_map(BuildPackage::cargo_args)
    }

    fn cargo_profile(&self) -> String {
        let profile = profile();
        if profile == "debug" {
            "dev".to_string()
        } else {
            profile
        }
    }

    fn cargo_config(&self) -> String {
        let home_dir = dirs::home_dir()
            .context("unable to get home directory")
            .unwrap();

        let cargo_dir = cargo_home_dir();
        let cargo_checkouts_dir = cargo_dir.join("git").join("checkouts");

        let config = [
            (&home_dir, "/home"),
            (&self.workspace_root, "/code"),
            (&cargo_dir, "/cargo"),
            (&cargo_checkouts_dir, "/deps"),
        ]
        .into_iter()
        .map(|(from, to)| format!("--remap-path-prefix={from}={to}", from = from.display()))
        .map(|flag| format!("\"{flag}\""))
        .collect::<Vec<String>>()
        .join(", ");

        // we set RUSTFLAGS via config because env vars reset flags we have in any `.cargo/config.toml`
        format!("target.wasm32-unknown-unknown.rustflags=[{config}]")
    }

    pub fn build(&mut self) {
        if get_no_build_env() || !self.rebuild_required() {
            println!("cargo:warning=Build skipped");
            return;
        }

        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut cargo = Command::new(cargo);

        if !get_no_map_remap_env() {
            cargo.arg("--config").arg(self.cargo_config());
        }

        cargo
            .arg("build")
            .arg("--no-default-features")
            .args(self.cargo_args())
            .arg("--profile")
            .arg(self.cargo_profile())
            .env(NO_BUILD_INNER_ENV, "1")
            .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
            .env("CARGO_TARGET_DIR", wasm_projects_dir())
            // remove host flags
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        println!("cargo:warning={:?}", cargo);
        let output = cargo.output().expect("Failed to execute cargo command");
        assert!(output.status.success());

        for pkg in &mut self.packages {
            if pkg.rebuild_kind == RebuildKind::Dirty {
                pkg.optimize();
            }
        }
    }

    pub fn wasm_binaries(&self) -> String {
        self.packages.iter().fold(String::new(), |mut output, pkg| {
            pkg.write_rust_mod(&mut output);
            output
        })
    }
}
