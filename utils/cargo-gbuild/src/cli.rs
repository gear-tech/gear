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

use crate::manifest;
use anyhow::{anyhow, Result};
use ccli::{clap, Parser};
use std::{fs, path::PathBuf, process::Command};

/// Command `gbuild` as cargo extension.
#[derive(Parser)]
pub struct GBuild {
    /// The path to the program manifest
    #[clap(short, long, default_value = "Cargo.toml")]
    pub manifest_path: PathBuf,

    /// Space or comma separated list of features to activate
    #[clap(short, long)]
    pub features: Vec<String>,

    /// Directory for all generated artifacts
    #[clap(short, long)]
    pub target_dir: Option<PathBuf>,

    /// If disable wasm-opt
    pub no_wasm_opt: bool,

    /// TODO: If enable meta build
    #[clap(short, long)]
    pub meta: bool,
}

impl GBuild {
    /// Build program
    pub fn build(&self) -> Result<()> {
        self.cargo()?;
        self.collect()
    }

    /// Process the cargo command.
    ///
    /// NOTE: only supports release build.
    fn cargo(&self) -> Result<()> {
        let mut cargo = Command::new("cargo");
        cargo.args(["build", "--release", "--target", "wasm32-unknown-unknown"]);
        cargo.args([
            "--manifest-path",
            self.manifest_path.to_string_lossy().to_string().as_str(),
        ]);
        if !self.features.is_empty() {
            cargo.args(["--features", &self.features.join(",")]);
        }

        if !cargo.status()?.success() {
            return Err(anyhow!("Failed to process the cargo command."));
        }

        Ok(())
    }

    // Collects the artifacts.
    fn collect(&self) -> Result<()> {
        let root = self
            .manifest_path
            .parent()
            .ok_or_else(|| anyhow!("Failed to parse the root directory."))?;
        let manifest =
            toml::from_str::<manifest::Manifest>(&fs::read_to_string(&self.manifest_path)?)?;
        let orgi_target_dir = manifest
            .build
            .and_then(|b| b.target_dir)
            .unwrap_or(root.join("target"));
        let name = manifest.package.name.replace('-', "_");
        let target_dir = self.target_dir.clone().unwrap_or(orgi_target_dir.clone());

        fs::create_dir_all(target_dir.join("gbuild"))?;

        // TODO: set the output of wasm opt as to the gbuild folder.
        fs::copy(
            orgi_target_dir.join(format!("wasm32-unknown-unknown/release/{name}.wasm")),
            target_dir.join(format!("gbuild/{name}.wasm")),
        )?;

        // TODO: process wasm-opt
        Ok(())
    }
}
