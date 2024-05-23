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

use crate::{artifact::Artifacts, metadata::Metadata, Artifact};
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
    pub fn run(self) -> Result<Artifacts> {
        let manifest_path = self
            .manifest_path
            .clone()
            .unwrap_or(etc::find_up("Cargo.toml")?);

        let metadata = Metadata::parse(manifest_path.clone(), self.features.clone())?;
        let cargo_target_dir: PathBuf = metadata.workspace.target_directory.clone().into();

        // Set up gbuild artifacts.
        let artifacts = Artifacts::new(
            self.target_dir
                .clone()
                .unwrap_or(cargo_target_dir.clone())
                .join(ARTIFACT_DIR),
            metadata,
        )?;

        // Run the cargo command.
        let (artifact, profile) = self.artifact_and_profile();
        let cargo_artifact_dir =
            cargo_target_dir.join(format!("wasm32-unknown-unknown/{}", artifact));
        self.cargo(profile, &cargo_target_dir, &manifest_path.clone())?;
        artifacts.process(cargo_artifact_dir)?;
        Ok(artifacts)
    }

    fn artifact_and_profile(&self) -> (String, Option<String>) {
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

            (
                if profile != DEV_PROFILE {
                    profile.clone()
                } else {
                    DEBUG_ARTIFACT.into()
                },
                Some(profile.into()),
            )
        } else if self.release {
            (RELEASE_PROFILE.into(), Some(RELEASE_PROFILE.into()))
        } else {
            (DEBUG_ARTIFACT.into(), None)
        }
    }

    /// Process the cargo command.
    fn cargo(
        &self,
        profile: Option<String>,
        cargo_target_dir: &Path,
        manifest_path: &Path,
    ) -> Result<()> {
        let mut kargo = CargoCommand::default();
        if let Some(profile) = profile {
            kargo.set_profile(profile);
        }

        kargo.set_manifest_path(manifest_path.to_path_buf());
        kargo.set_target_dir(cargo_target_dir.to_path_buf());
        kargo.set_features(&self.features);
        kargo.run()
    }
}
