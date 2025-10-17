// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cargo_toolchain::Toolchain;
use anyhow::{Context, Result, anyhow, ensure};
use std::{env, path::PathBuf, process::Command};

/// Helper to deal with the `cargo` command.
#[derive(Clone)]
pub struct CargoCommand {
    path: String,
    manifest_path: PathBuf,
    profile: String,
    rustc_flags: Vec<&'static str>,
    target_dir: PathBuf,
    features: Vec<String>,
    toolchain: Toolchain,
    check_recommended_toolchain: bool,
    force_recommended_toolchain: bool,
}

impl CargoCommand {
    /// Initialize new cargo command.
    pub fn new() -> CargoCommand {
        let toolchain = Toolchain::try_from_rustup().expect("Failed to resolve toolchain version");

        CargoCommand {
            path: "rustup".to_string(),
            manifest_path: "Cargo.toml".into(),
            profile: "dev".to_string(),
            // TODO: enable `-C linker-plugin-lto` (https://github.com/rust-lang/rust/issues/130604)
            rustc_flags: vec!["-C", "link-arg=--import-memory"],
            target_dir: "target".into(),
            features: vec![],
            toolchain,
            check_recommended_toolchain: false,
            force_recommended_toolchain: false,
        }
    }
}

impl Default for CargoCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CargoCommand {
    /// Set path to the `Cargo.toml` file.
    pub fn set_manifest_path(&mut self, path: PathBuf) {
        self.manifest_path = path;
    }

    /// Set path to the `target` directory.
    pub fn set_target_dir(&mut self, path: PathBuf) {
        self.target_dir = path;
    }

    /// Set profile.
    ///
    /// Possible values: `dev`, `release`.
    pub fn set_profile(&mut self, profile: String) {
        self.profile = profile;
    }

    /// Set features.
    pub fn set_features(&mut self, features: &[String]) {
        self.features = features.into();
    }

    /// Sets whether to check the version of the recommended toolchain.
    pub fn set_check_recommended_toolchain(&mut self, check_recommended_toolchain: bool) {
        self.check_recommended_toolchain = check_recommended_toolchain;
    }

    /// Sets whether to force the version of the recommended toolchain.
    pub fn set_force_recommended_toolchain(&mut self, force_recommended_toolchain: bool) {
        self.force_recommended_toolchain = force_recommended_toolchain;
    }

    /// Execute the `cargo` command with invoking supplied arguments.
    pub fn run(&self) -> Result<()> {
        if self.check_recommended_toolchain {
            self.toolchain.check_recommended_toolchain()?;
        }

        let toolchain = if self.force_recommended_toolchain {
            Toolchain::recommended_nightly()
        } else {
            self.toolchain.clone()
        };

        let mut cargo = Command::new(&self.path);
        cargo
            .arg("run")
            .arg(toolchain.raw_toolchain_str().as_ref())
            .arg("cargo");

        if self.force_recommended_toolchain {
            self.clean_up_environment(&mut cargo);
        }

        cargo
            .arg("rustc")
            .arg("--target=wasm32v1-none")
            .arg("--color=always")
            .arg(format!("--manifest-path={}", self.manifest_path.display()))
            .arg("--profile")
            .arg(&self.profile);

        if !self.features.is_empty() {
            cargo.arg("--features");
            cargo.arg(self.features.join(","));
        }

        cargo
            .arg("--")
            .args(&self.rustc_flags)
            .env("CARGO_TARGET_DIR", &self.target_dir)
            .env("__GEAR_WASM_BUILDER_NO_BUILD", "1"); // Don't build the original crate recursively

        self.remove_cargo_encoded_rustflags(&mut cargo);

        cargo.env("CARGO_ENCODED_RUSTFLAGS", "-Ctarget-feature=+sign-ext");

        let status = cargo.status().context("unable to execute cargo command")?;
        ensure!(
            status.success(),
            anyhow!("cargo command run failed: {status}")
        );

        Ok(())
    }

    fn clean_up_environment(&self, command: &mut Command) {
        // Inherited build script environment variables must be removed
        // so that they cannot change the behavior of the cargo package manager.

        // https://doc.rust-lang.org/cargo/reference/environment-variables.html
        // `RUSTC_WRAPPER` and `RUSTC_WORKSPACE_WRAPPER` are not removed due to tools like sccache.
        const INHERITED_ENV_VARS: &[&str] = &[
            "CARGO",
            "CARGO_MANIFEST_DIR",
            "CARGO_MANIFEST_LINKS",
            "CARGO_MAKEFLAGS",
            "OUT_DIR",
            "TARGET",
            "HOST",
            "NUM_JOBS",
            "OPT_LEVEL",
            "PROFILE",
            "RUSTC",
            "RUSTDOC",
            "RUSTC_LINKER",
            "CARGO_ENCODED_RUSTFLAGS",
        ];

        for env_var in INHERITED_ENV_VARS {
            command.env_remove(env_var);
        }

        const INHERITED_ENV_VARS_WITH_PREFIX: &[&str] =
            &["CARGO_FEATURE_", "CARGO_CFG_", "DEP_", "CARGO_PKG_"];

        for (env_var, _) in env::vars() {
            for prefix in INHERITED_ENV_VARS_WITH_PREFIX {
                if env_var.starts_with(prefix) {
                    command.env_remove(&env_var);
                }
            }
        }
    }

    fn remove_cargo_encoded_rustflags(&self, command: &mut Command) {
        // substrate's wasm-builder removes these vars so do we
        // check its source for details
        command.env_remove("CARGO_ENCODED_RUSTFLAGS");
        command.env_remove("RUSTFLAGS");
    }
}
