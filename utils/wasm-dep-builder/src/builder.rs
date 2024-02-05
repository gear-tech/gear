// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
    lock::{BinariesLockFileConfig, LockFileConfig, ProgramLockFileConfig},
    utils::{
        cargo_home_dir, crate_target_dir, get_no_build_env, get_no_path_remap_env, profile,
        wasm32_target_dir, WASM32_TARGET,
    },
    UnderscoreString, NO_BUILD_INNER_ENV,
};
use anyhow::Context;
use cargo_metadata::Package;
use gear_wasm_builder::{
    optimize,
    optimize::{OptType, Optimizer},
};
use std::{
    collections::BTreeSet,
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

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
    wasm_bloaty: PathBuf,
    wasm: PathBuf,
}

impl BuildPackage {
    pub fn new(pkg: &Package, config: LockFileConfig, excluded_features: BTreeSet<String>) -> Self {
        let name = UnderscoreString(pkg.name.clone());
        let (rebuild_kind, features) = Self::resolve_features(pkg, config, excluded_features);
        let (wasm_bloaty, wasm) = Self::wasm_paths(&name);

        Self {
            name,
            rebuild_kind,
            features,
            wasm_bloaty,
            wasm,
        }
    }

    fn resolve_features(
        pkg: &Package,
        config: LockFileConfig,
        excluded_features: BTreeSet<String>,
    ) -> (RebuildKind, BTreeSet<String>) {
        match config {
            LockFileConfig::Program(ProgramLockFileConfig { features }) => {
                // make full list of excluded features
                let excluded_features = excluded_features
                    .into_iter()
                    .map(UnderscoreString)
                    .chain(
                        DEFAULT_EXCLUDED_FEATURES
                            .map(str::to_string)
                            .map(UnderscoreString),
                    )
                    .collect();

                // actually exclude features from list of enabled features
                let features: BTreeSet<UnderscoreString> =
                    features.difference(&excluded_features).cloned().collect();

                // collect all of the features with their original names
                let orig_features: BTreeSet<UnderscoreString> =
                    pkg.features.keys().cloned().map(UnderscoreString).collect();

                // get original names of enabled features
                // because list is built from `CARGO_FEATURE_*` env vars (underscored names)
                // in given config
                let features: BTreeSet<String> = orig_features
                    .intersection(&features)
                    .cloned()
                    .map(|s| s.0)
                    .collect();

                // if config type is `Program` it's anyway has to be built
                // because such config is written in case of any changes occurred
                (RebuildKind::Dirty, features)
            }
            LockFileConfig::Binaries(BinariesLockFileConfig { features }) => {
                // if config type is `Binaries` then no changes were made in program
                // and builder have already done its job
                (RebuildKind::Fresh, features)
            }
        }
    }

    fn wasm_paths(name: &UnderscoreString) -> (PathBuf, PathBuf) {
        let wasm32_target_dir = wasm32_target_dir().join(profile());
        let wasm_bloaty = wasm32_target_dir.join(format!("{name}.wasm"));
        let mut wasm = wasm_bloaty.clone();
        wasm.set_extension("opt.wasm");
        (wasm_bloaty, wasm)
    }

    pub fn name(&self) -> &UnderscoreString {
        &self.name
    }

    pub fn features(&self) -> &BTreeSet<String> {
        &self.features
    }

    pub fn wasm_bloaty_path(&self) -> &Path {
        self.wasm_bloaty.as_path()
    }

    pub fn wasm_path(&self) -> &Path {
        self.wasm.as_path()
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
        let (wasm_bloaty, wasm) = (&self.wasm_bloaty, &self.wasm);

        optimize::optimize_wasm(wasm_bloaty.clone(), wasm.clone(), "4", true)
            .with_context(|| {
                format!(
                    "failed to optimize {wasm_bloaty}",
                    wasm_bloaty = wasm_bloaty.display()
                )
            })
            .unwrap();

        let mut optimizer = Optimizer::new(wasm.clone()).unwrap();
        optimizer.insert_stack_end_export().unwrap_or_else(|err| {
            println!(
                "cargo:warning=Cannot insert stack end export into `{name}`: {err}",
                name = self.name.original()
            )
        });
        optimizer.strip_custom_sections();

        let binary_opt = optimizer.optimize(OptType::Opt).unwrap();
        fs::write(wasm, binary_opt).unwrap();
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

    fn skip_build(&self) -> bool {
        let any_dirty = self
            .packages
            .iter()
            .any(|pkg| pkg.rebuild_kind == RebuildKind::Dirty);

        get_no_build_env() || !any_dirty
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

        // we set additional RUSTFLAGS via config because env vars reset flags we have in any `.cargo/config.toml`
        format!("target.wasm32-unknown-unknown.rustflags=[{config}]")
    }

    pub fn build(&mut self) -> bool {
        if self.skip_build() {
            return false;
        }

        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut cargo = Command::new(cargo);

        if !get_no_path_remap_env() {
            cargo.arg("--config").arg(self.cargo_config());
        }

        cargo
            .arg("build")
            .arg("--no-default-features")
            .args(self.cargo_args())
            .arg("--profile")
            .arg(self.cargo_profile())
            .env(NO_BUILD_INNER_ENV, "1")
            .env("CARGO_BUILD_TARGET", WASM32_TARGET)
            .env("CARGO_TARGET_DIR", crate_target_dir())
            // remove host flags
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        let output = cargo.output().expect("Failed to execute cargo command");
        if !output.status.success() {
            let _ = io::stderr().write_all(&output.stderr);
            panic!("{}", output.status);
        }

        for pkg in &mut self.packages {
            if pkg.rebuild_kind == RebuildKind::Dirty {
                pkg.optimize();
            }
        }

        true
    }
}
