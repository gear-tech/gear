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

const DEV_PROFILE: &str = "dev";
const DEBUG_ARTIFACT: &str = "debug";
const RELEASE_PROFILE: &str = "release";

/// Command `gbuild` as cargo extension.
#[derive(Parser, Default)]
pub struct GBuild {
    /// Space or comma separated list of features to activate
    #[clap(short = 'F', long)]
    pub features: Vec<String>,

    /// The path to the program manifest
    #[clap(short, long)]
    pub manifest_path: Option<PathBuf>,

    /// Build artifacts with the specified profile
    #[clap(long)]
    pub profile: Option<String>,

    /// If enables the release profile.
    #[clap(short, long)]
    pub release: bool,

    /// Directory for all generated artifacts
    ///
    /// If not set, the default value will be the target folder
    /// of the cargo project.
    #[clap(short, long)]
    pub target_dir: Option<PathBuf>,

    /// If enable workspace build
    #[clap(short, long)]
    pub workspace: bool,
}

impl GBuild {
    /// Set manifest and return self
    pub fn manifest_path(mut self, manifest: PathBuf) -> Self {
        self.manifest_path = Some(manifest);
        self
    }

    /// Set workspace with true
    pub fn workspace(mut self) -> Self {
        self.workspace = true;
        self
    }

    /// Run the gbuild command
    pub fn run(&self) -> Result<Artifacts> {
        let manifest_path = self
            .manifest_path
            .clone()
            .unwrap_or(etc::find_up("Cargo.toml")?);

        let (artifact, profile) = self.artifact_and_profile();
        let metadata =
            Metadata::parse(self.workspace, manifest_path.clone(), self.features.clone())?;
        let target_dir = self
            .target_dir
            .clone()
            .unwrap_or(metadata.target_directory.clone().into());

        // 1. setup cargo command
        let mut kargo = CargoCommand::default();
        kargo.set_features(&self.features);
        kargo.set_target_dir(target_dir.clone());
        if let Some(profile) = profile {
            kargo.set_profile(profile);
        }

        // 2. setup gbuild artifacts.
        let artifacts = Artifacts::new(
            target_dir.join("gbuild"),
            target_dir.join("wasm32-unknown-unknown").join(artifact),
            metadata,
            kargo,
        )?;

        // 3. process artifacts
        artifacts.process()?;
        Ok(artifacts)
    }

    fn artifact_and_profile(&self) -> (String, Option<String>) {
        let mut artifact = DEBUG_ARTIFACT.to_string();
        let mut profile: Option<String> = None;

        if let Some(p) = &self.profile {
            if self.release {
                eprintln!(
                    "{}: conflicting usage of --profile={} and --release
The `--release` flag is the same as `--profile=release`.
Remove one flag or the other to continue.",
                    "error".red().bold(),
                    p
                );
                std::process::exit(1);
            }

            profile = Some(p.to_string());
            if p != DEV_PROFILE {
                artifact = p.into()
            }
        } else if self.release {
            artifact = RELEASE_PROFILE.into();
            profile = Some(artifact.clone());
        }

        (artifact, profile)
    }
}
