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
    profile, wasm32_target_dir, wasm_projects_dir, BuilderLockFile, BuilderLockFileConfig,
    LockFile, UnderscoreString,
};
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

#[derive(Debug, Eq, PartialEq)]
pub enum RebuildKind {
    Changed,
    Still,
}

#[derive(Debug)]
pub struct BuildPackage {
    pub rebuild_kind: RebuildKind,
    pub features: BTreeSet<String>,
    pub lock: LockFile<BuilderLockFile>,
}

#[derive(Debug, Default)]
pub struct BuildPackages {
    packages: BTreeMap<UnderscoreString, BuildPackage>,
}

impl BuildPackages {
    pub fn insert(&mut self, pkg_name: String, pkg: BuildPackage) {
        self.packages.insert(UnderscoreString(pkg_name), pkg);
    }

    fn rebuild_required(&self) -> bool {
        self.packages
            .values()
            .any(|pkg| pkg.rebuild_kind == RebuildKind::Changed)
    }

    fn cargo_args(&self) -> impl Iterator<Item = String> + '_ {
        self.packages
            .iter()
            .map(|(pkg_name, pkg)| {
                (
                    pkg_name.original().clone(),
                    pkg.features
                        .iter()
                        .map(|feature| format!("{}/{feature}", pkg_name.original()))
                        .collect::<Vec<_>>()
                        .join(","),
                )
            })
            .flat_map(|(pkg, features)| {
                [
                    "--package".to_string(),
                    pkg,
                    "--features".to_string(),
                    features,
                ]
            })
    }

    fn wasm_paths(pkg_name: &UnderscoreString) -> (PathBuf, PathBuf) {
        let wasm32_target_dir = wasm32_target_dir().join(profile());
        let wasm = wasm32_target_dir.join(format!("{pkg_name}.wasm"));
        let mut wasm_opt = wasm.clone();
        wasm_opt.set_extension("opt.wasm");
        (wasm, wasm_opt)
    }

    fn optimize(&self) {
        for (pkg_name, _pkg) in self
            .packages
            .iter()
            .filter(|(_, pkg)| pkg.rebuild_kind == RebuildKind::Changed)
        {
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
    }

    fn write_configs(&mut self) {
        for (_, pkg) in self.packages.iter_mut() {
            pkg.lock.write(BuilderLockFileConfig {
                features: pkg.features.clone(),
            })
        }
    }

    pub fn build(&mut self) {
        if !self.rebuild_required() {
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
            .env("__WASM_DEP_BUILDER_NO_BUILD", "1")
            .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
            .env("CARGO_TARGET_DIR", wasm_projects_dir())
            // remove host flags
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        println!("cargo:warning={:?}", cargo);
        let output = cargo.output().expect("Failed to execute cargo command");
        assert!(output.status.success());

        self.optimize();
        self.write_configs();
    }

    pub fn wasm_binaries(&self) -> String {
        self.packages
            .iter()
            .fold(String::new(), |mut output, (pkg_name, _pkg)| {
                let (wasm, wasm_opt) = Self::wasm_paths(pkg_name);
                let _ = write!(
                    &mut output,
                    r#"
pub mod {pkg_name} {{
    pub use ::{pkg_name}::*;
    
    pub const WASM_BINARY_BLOATY: &[u8] = include_bytes!("{}");
    pub const WASM_BINARY: &[u8] = include_bytes!("{}");
}}
                    "#,
                    wasm.display(),
                    wasm_opt.display()
                );
                output
            })
    }
}
