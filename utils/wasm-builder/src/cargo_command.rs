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

use anyhow::{ensure, Result};
use std::{env, path::PathBuf, process::Command};

use crate::builder_error::BuilderError;

/// Helper to deal with the `cargo` command.
pub struct CargoCommand {
    path: String,
    manifest_path: PathBuf,
    args: Vec<&'static str>,
    profile: String,
    rustc_flags: Vec<&'static str>,
}

impl CargoCommand {
    /// Create a new `CargoCommand`.
    pub fn new() -> Self {
        CargoCommand {
            path: "cargo".to_string(),
            manifest_path: "Cargo.toml".into(),
            args: vec!["+nightly", "rustc", "--target=wasm32-unknown-unknown"],
            profile: "dev".to_string(),
            rustc_flags: vec!["-C", "link-arg=--import-memory", "-C", "linker-plugin-lto"],
        }
    }

    /// Set path to the `Cargo.toml` file.
    pub fn set_manifest_path(&mut self, path: PathBuf) {
        self.manifest_path = path;
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
            .arg("--release")
            .arg("--")
            .args(&self.rustc_flags)
            .env(self.skip_build_env(), ""); // Don't build the original crate recursively

        self.set_cargo_encoded_rustflags(&mut cargo);

        let status = cargo.status()?;
        ensure!(
            status.success(),
            BuilderError::CargoRunFailed(status.to_string())
        );

        Ok(())
    }

    /// Generate a project specific environment variable that used to skip the build.
    pub fn skip_build_env(&self) -> String {
        format!(
            "SKIP_{}_WASM_BUILD",
            env::var("CARGO_PKG_NAME")
                .expect("Package name is set")
                .to_uppercase()
                .replace('-', "_"),
        )
    }

    fn set_cargo_encoded_rustflags(&self, command: &mut Command) {
        const RUSTFLAGS: &str = "CARGO_ENCODED_RUSTFLAGS";

        let rustflags = env::var(RUSTFLAGS)
            .expect("`CARGO_ENCODED_RUSTFLAGS` is always set in build scripts")
            .replace("-Cinstrument-coverage", "");
        command.env(RUSTFLAGS, rustflags);
    }
}
