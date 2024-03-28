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
use gear_wasm_builder::optimize::{self, OptType, Optimizer};
use std::{fs, path::PathBuf, process::Command};

const ARTIFACT_DIR: &str = "gbuild";

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
    pub fn build(&self) -> Result<Artifact> {
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
    fn collect(&self) -> Result<Artifact> {
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

        // Register the path of the built artifacts.
        let name = manifest.package.name.replace('-', "_");
        let target_dir = self
            .target_dir
            .clone()
            .unwrap_or(orgi_target_dir.clone())
            .join(ARTIFACT_DIR);
        let artifact = Artifact {
            program: target_dir.join(format!("{name}.wasm")),
            // TODO: support meta build
            //
            // target_dir.join(format!("{name}.meta.wasm")),
            meta: None,
            root: target_dir,
        };

        // Optimize the built wasm and return the artifact
        artifact
            .process(orgi_target_dir.join(format!("wasm32-unknown-unknown/release/{name}.wasm")))?;
        Ok(artifact)
    }
}

/// Artifact registry
///
/// This instance simply holds the paths of the built binaries
/// for re-using stuffs.
pub struct Artifact {
    /// The path of the root artifact
    pub root: PathBuf,
    /// The path to the built program.
    pub program: PathBuf,
    /// The path to the built metadata.
    pub meta: Option<PathBuf>,
}

impl Artifact {
    /// Build artifacts with optimization.
    pub fn process(&self, src: PathBuf) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        optimize::optimize_wasm(src, self.program.clone(), "4", true)?;
        let mut optimizer = Optimizer::new(self.program.clone())?;
        optimizer
            .insert_stack_end_export()
            .map_err(|e| anyhow!(e))?;
        optimizer.strip_custom_sections();
        fs::write(self.program.clone(), optimizer.optimize(OptType::Opt)?).map_err(Into::into)
    }
}
