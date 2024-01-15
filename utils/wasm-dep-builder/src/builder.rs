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
    get_no_build_env,
    lock::{BuilderLockFile, BuilderLockFileConfig, DemoLockFileConfig, LockFileConfig},
    profile, wasm32_target_dir, wasm_projects_dir, UnderscoreString, NO_BUILD_INNER_ENV,
};
use cargo_metadata::Package;
use gear_wasm_builder::{
    optimize,
    optimize::{OptType, Optimizer},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fmt::Write,
    fs,
    path::PathBuf,
    process::Command,
};

const DEFAULT_EXCLUDED_FEATURES: [&str; 3] = ["default", "std", "wasm-wrapper"];

#[derive(Debug, Eq, PartialEq)]
enum RebuildKind {
    Fresh,
    Dirty,
}

#[derive(Debug)]
struct BuildPackage {
    rebuild_kind: RebuildKind,
    features: BTreeSet<String>,
    lock: BuilderLockFile,
}

impl BuildPackage {
    fn new(pkg: &Package, mut lock: BuilderLockFile, excluded_features: BTreeSet<String>) -> Self {
        let config = lock.read();
        let (rebuild_kind, features) = Self::resolve_features(pkg, config, excluded_features);

        Self {
            rebuild_kind,
            features,
            lock,
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

    fn wasm_paths(pkg_name: &UnderscoreString) -> (PathBuf, PathBuf) {
        let wasm32_target_dir = wasm32_target_dir().join(profile());
        let wasm = wasm32_target_dir.join(format!("{pkg_name}.wasm"));
        let mut wasm_opt = wasm.clone();
        wasm_opt.set_extension("opt.wasm");
        (wasm, wasm_opt)
    }

    fn cargo_args(&self, pkg_name: &UnderscoreString) -> impl Iterator<Item = String> {
        let pkg_name = pkg_name.original().clone();
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

    fn optimize(&self, pkg_name: &UnderscoreString) {
        let (wasm, wasm_opt) = Self::wasm_paths(pkg_name);

        optimize::optimize_wasm(wasm.clone(), wasm_opt.clone(), "4", true).unwrap();

        let mut optimizer = Optimizer::new(wasm_opt.clone()).unwrap();
        optimizer.insert_stack_end_export().unwrap_or_else(|err| {
            println!("cargo:warning=Cannot insert stack end export into `{pkg_name}`: {err}")
        });
        optimizer.strip_custom_sections();

        let binary_opt = optimizer.optimize(OptType::Opt).unwrap();
        fs::write(&wasm_opt, binary_opt).unwrap();
    }

    fn write_config(&mut self) {
        let config = BuilderLockFileConfig {
            features: self.features.clone(),
        };
        self.lock.write(config);
    }

    fn write_rust_mod(&self, pkg_name: &UnderscoreString, output: &mut String) {
        let (wasm_bloaty, wasm) = if get_no_build_env() {
            ("&[]".to_string(), "&[]".to_string())
        } else {
            let (wasm_bloaty, wasm) = Self::wasm_paths(pkg_name);
            (
                format!(r#"include_bytes!("{}")"#, wasm_bloaty.display()),
                format!(r#"include_bytes!("{}")"#, wasm.display()),
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

#[derive(Debug, Default)]
pub struct BuildPackages {
    packages: BTreeMap<UnderscoreString, BuildPackage>,
}

impl BuildPackages {
    pub fn insert(
        &mut self,
        pkg: &Package,
        lock: BuilderLockFile,
        excluded_features: BTreeSet<String>,
    ) {
        self.packages.insert(
            UnderscoreString(pkg.name.clone()),
            BuildPackage::new(pkg, lock, excluded_features),
        );
    }

    fn rebuild_required(&self) -> bool {
        self.packages
            .values()
            .any(|pkg| pkg.rebuild_kind == RebuildKind::Dirty)
    }

    fn cargo_args(&self) -> impl Iterator<Item = String> + '_ {
        self.packages
            .iter()
            .flat_map(|(pkg_name, pkg)| pkg.cargo_args(pkg_name))
    }

    pub fn build(&mut self) {
        if get_no_build_env() || !self.rebuild_required() {
            println!("cargo:warning=Build skipped");
            return;
        }

        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut cargo = Command::new(cargo);
        cargo
            .arg("build")
            .arg("--no-default-features")
            .args(self.cargo_args())
            .arg("--profile")
            .arg(profile().replace("debug", "dev"))
            .arg("-v")
            .env(NO_BUILD_INNER_ENV, "1")
            .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
            .env("CARGO_TARGET_DIR", wasm_projects_dir())
            // remove host flags
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        println!("cargo:warning={:?}", cargo);
        let output = cargo.output().expect("Failed to execute cargo command");
        assert!(output.status.success());

        for (name, pkg) in &mut self.packages {
            if pkg.rebuild_kind == RebuildKind::Dirty {
                pkg.optimize(name);
            }

            pkg.write_config();
        }
    }

    pub fn wasm_binaries(&self) -> String {
        self.packages
            .iter()
            .fold(String::new(), |mut output, (pkg_name, pkg)| {
                pkg.write_rust_mod(pkg_name, &mut output);
                output
            })
    }
}
