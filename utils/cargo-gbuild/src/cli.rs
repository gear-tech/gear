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

//! TODO: Introduce a standard for the project structure of gear programs (#3866)

use crate::{metadata::Metadata, Artifact};
use anyhow::{anyhow, Result};
use cargo_toml::Manifest;
use clap::Parser;
use colored::Colorize;
use gear_wasm_builder::CargoCommand;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const ARTIFACT_DIR: &str = "gbuild";
const DEV_PROFILE: &str = "dev";
const DEBUG_ARTIFACT: &str = "debug";
const RELEASE_PROFILE: &str = "release";

/// Command `gbuild` as cargo extension.
#[derive(Parser)]
pub struct GBuild {
    /// The path to the program manifest
    #[clap(short, long)]
    pub manifest_path: Option<PathBuf>,

    /// Space or comma separated list of features to activate
    #[clap(short = 'F', long)]
    pub features: Vec<String>,

    /// If enables the release profile.
    #[clap(short, long)]
    pub release: bool,

    /// Directory for all generated artifacts
    ///
    /// If not set, the default value will be the target folder
    /// of the cargo project.
    #[clap(short, long)]
    pub target_dir: Option<PathBuf>,

    /// Build artifacts with the specified profile
    #[clap(long)]
    pub profile: Option<String>,
}

impl GBuild {
    /// Run the gbuild command
    ///
    /// TODO: Support `gtest::Program::current` (#3851)
    pub fn run(self) -> Result<Artifact> {
        let manifest_path = self
            .manifest_path
            .clone()
            .unwrap_or(etc::find_up("Cargo.toml")?);

        let metadata = Metadata::parse(manifest_path.clone(), self.features.clone())?;
        let cargo_target_dir: PathBuf = metadata.workspace.target_directory.into();

        // Run the cargo command, process optimizations and collect artifacts.
        let artifact = self.cargo(&manifest_path.clone(), &cargo_target_dir)?;
        let gbuild_artifact_dir = self
            .target_dir
            .unwrap_or(cargo_target_dir.clone())
            .join(ARTIFACT_DIR);

        let artifact = Artifact::new(
            gbuild_artifact_dir,
            Manifest::from_path(manifest_path)?.package().name.as_ref(),
        )?;
        artifact.process(cargo_target_dir)?;
        Ok(artifact)
    }

    /// Process the cargo command.
    fn cargo(&self, manifest_path: &Path, cargo_target_dir: &Path) -> Result<PathBuf> {
        let mut kargo = CargoCommand::default();
        let mut artifact = DEBUG_ARTIFACT;

        if let Some(profile) = &self.profile {
            if self.release {
                eprintln!(
                    "{}: conflicting usage of --profile={} and --release
The `--release` flag is the same as `--profile=release`.
Remove one flag or the other to continue.",
                    "error".red().bold(),
                    profile
                );
                std::process::exit(1);
            }

            kargo.set_profile(profile.clone());
            if profile != DEV_PROFILE {
                artifact = profile;
            }
        } else if self.release {
            kargo.set_profile(RELEASE_PROFILE.into());
            artifact = RELEASE_PROFILE;
        }

        kargo.set_manifest_path(manifest_path.to_path_buf());
        kargo.set_target_dir(cargo_target_dir.to_path_buf());
        kargo.set_features(&self.features);
        kargo.run()?;

        // Returns the root of the built artifact
        Ok(cargo_target_dir.join(format!("wasm32-unknown-unknown/{}", artifact)))
    }
}
