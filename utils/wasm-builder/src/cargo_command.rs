// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use anyhow::{ensure, Context, Result};
use std::{path::PathBuf, process::Command};

use crate::builder_error::BuilderError;

/// Helper to deal with the `cargo` command.
pub struct CargoCommand {
    path: String,
    manifest_path: PathBuf,
    args: Vec<&'static str>,
    profile: String,
    rustc_flags: Vec<&'static str>,
    target_dir: PathBuf,
}

impl CargoCommand {
    /// Create a new `CargoCommand`.
    pub fn new() -> Self {
        CargoCommand {
            path: "rustup".to_string(),
            manifest_path: "Cargo.toml".into(),
            args: vec![
                "run",
                "nightly",
                "cargo",
                "rustc",
                "--target=wasm32-unknown-unknown",
            ],
            profile: "dev".to_string(),
            rustc_flags: vec!["-C", "link-arg=--import-memory", "-C", "linker-plugin-lto"],
            target_dir: "target".into(),
        }
    }

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

    /// Execute the `cargo` command with invoking supplied arguments.
    pub fn run(&self) -> Result<()> {
        let mut cargo = Command::new(&self.path);
        cargo
            .args(&self.args)
            .arg("--color=always")
            .arg(format!("--manifest-path={}", self.manifest_path.display()))
            .arg("--profile")
            .arg(&self.profile)
            .arg("--")
            .args(&self.rustc_flags)
            .env("CARGO_TARGET_DIR", &self.target_dir)
            .env("__GEAR_WASM_BUILDER_NO_BUILD", "1"); // Don't build the original crate recursively

        self.remove_cargo_encoded_rustflags(&mut cargo);

        let status = cargo.status().context("unable to execute cargo command")?;
        ensure!(
            status.success(),
            BuilderError::CargoRunFailed(status.to_string())
        );

        Ok(())
    }

    fn remove_cargo_encoded_rustflags(&self, command: &mut Command) {
        // substrate's wasm-builder removes these vars so do we
        // check its source for details
        command.env_remove("CARGO_ENCODED_RUSTFLAGS");
        command.env_remove("RUSTFLAGS");
    }
}
