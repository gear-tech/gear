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

use crate::{
    builder_error::BuilderError,
    crate_info::CrateInfo,
    sys::{self, FeatureSetBuilder, Toolchain},
};
use anyhow::{ensure, Context, Result};
use std::{path::PathBuf, process::Command};

/// Helper to deal with the `cargo` command.
pub struct CargoCommand {
    path: String,
    manifest_path: PathBuf,
    args: Vec<String>,
    profile: String,
    rustc_flags: Vec<&'static str>,
    target_dir: PathBuf,
    binary_features: Vec<&'static str>,
}

impl CargoCommand {
    /// Create a new `CargoCommand`.
    pub fn new(binary_features: Vec<&'static str>) -> Self {
        let toolchain = sys::with_current_command(|cmd| {
            Toolchain::from_command(cmd).into_nightly_toolchain_string()
        });

        let mut args = [
            "run",
            // {toolchain} to be placed below
            "cargo",
            "rustc",
            "--target=wasm32-unknown-unknown",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

        args.insert(1, toolchain);

        CargoCommand {
            path: "rustup".to_string(),
            manifest_path: "Cargo.toml".into(),
            args,
            profile: "dev".to_string(),
            rustc_flags: vec!["-C", "link-arg=--import-memory", "-C", "linker-plugin-lto"],
            target_dir: "target".into(),
            binary_features,
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
            .arg(&self.profile);

        let mut crate_info = CrateInfo::from_manifest(self.manifest_path.as_path(), false)?;
        let mut feature_set = sys::with_current_command(|cmd| {
            let feature_set_builder = FeatureSetBuilder::from_crate_name(crate_info.name);
            feature_set_builder.build_from_command(cmd)
        });

        let default_features = crate_info.features.remove("default");
        let features = crate_info.features.into_keys().collect::<Vec<_>>();

        cargo.arg("--no-default-features");
        let all_features = feature_set.all_features();

        if all_features {
            feature_set.convert_all_features(features.clone());
        };

        if all_features || !feature_set.no_default_features() {
            if let Some(default) = default_features {
                default.into_iter().for_each(|f| feature_set.add_feature(f));
            }
        }

        self.binary_features
            .iter()
            .for_each(|f| feature_set.remove_feature(f));

        feature_set.filter_existing(features);

        if let Some(features_string) = feature_set.features_string() {
            cargo.arg("--features").arg(features_string);
        }

        cargo
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
