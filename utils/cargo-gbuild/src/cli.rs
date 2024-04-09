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

use crate::Artifact;
use anyhow::{anyhow, Result};
use cargo_toml::Manifest;
use clap::Parser;
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
    #[clap(short, long, default_value = "Cargo.toml")]
    pub manifest_path: PathBuf,

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
        // 1. Get the cargo target directory
        //
        // TODO: Detect if the package is part of a workspace. (#3852)
        // TODO: Support target dir defined in `.cargo/config.toml` (#3852)
        let absolute_root = fs::canonicalize(self.manifest_path.clone())?;
        let cargo_target_dir = env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or(
            absolute_root
                .parent()
                .ok_or_else(|| anyhow!("Failed to parse the root directory."))?
                .join("target"),
        );

        // 2. Run the cargo command, process optimizations and collect artifacts.
        let cargo_artifact_dir = self.cargo(&cargo_target_dir)?;
        let gbuild_artifact_dir = self
            .target_dir
            .unwrap_or(cargo_target_dir.clone())
            .join(ARTIFACT_DIR);

        let artifact = Artifact::new(
            gbuild_artifact_dir,
            Manifest::from_path(self.manifest_path.clone())?
                .package()
                .name
                .replace('-', "_"),
        )?;
        artifact.process(cargo_artifact_dir)?;
        Ok(artifact)
    }

    /// Process the cargo command.
    ///
    /// TODO: Support workspace build. (#3852)
    fn cargo(&self, target_dir: &Path) -> Result<PathBuf> {
        let mut kargo = CargoCommand::default();
        let mut artifact = DEBUG_ARTIFACT;

        // NOTE: If profile is provided, ignore the release flag.
        if let Some(profile) = &self.profile {
            kargo.set_profile(profile.clone());
            if profile != DEV_PROFILE {
                artifact = profile;
            }
        } else if self.release {
            kargo.set_profile(RELEASE_PROFILE.into());
            artifact = RELEASE_PROFILE;
        }

        kargo.set_manifest_path(self.manifest_path.clone());
        kargo.set_target_dir(target_dir.to_path_buf());
        kargo.set_features(&self.features);
        kargo.run()?;

        // Returns the root of the built artifact
        Ok(target_dir.join(format!("wasm32-unknown-unknown/{}", artifact)))
    }
}
