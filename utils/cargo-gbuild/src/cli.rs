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
use gear_wasm_builder::{
    optimize::{self, OptType, Optimizer},
    CargoCommand,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

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

    /// If enables the release profile.
    #[clap(short, long)]
    pub release: bool,

    /// Directory for all generated artifacts
    ///
    /// If not set, the default value will be the target folder
    /// of the cargo project.
    #[clap(short, long)]
    pub target_dir: Option<PathBuf>,

    /// TODO: If enable meta build
    #[clap(short, long)]
    pub meta: bool,
}

impl GBuild {
    // Collects the artifacts.
    ///
    /// TODO: generate `wasm_binary.rs` in the output directory.
    pub fn collect(&self) -> Result<Artifact> {
        let root = self
            .manifest_path
            .parent()
            .ok_or_else(|| anyhow!("Failed to parse the root directory."))?;

        let manifest =
            toml::from_str::<manifest::Manifest>(&fs::read_to_string(&self.manifest_path)?)
                .map_err(|e| anyhow!("Failed to find the manifest file, {e}"))?;
        let cargo_target_dir = manifest
            .build
            .and_then(|b| b.target_dir)
            .unwrap_or(root.join("target"));

        // Register the path of the built artifacts.
        let name = manifest.package.name.replace('-', "_");
        let gbuild_target_dir = self
            .target_dir
            .clone()
            .unwrap_or(cargo_target_dir.clone())
            .join(ARTIFACT_DIR);

        self.cargo(&cargo_target_dir)?;
        let artifact = Artifact {
            program: gbuild_target_dir.join(format!("{name}.wasm")),
            // TODO: support meta build
            //
            // target_dir.join(format!("{name}.meta.wasm")),
            meta: None,
            root: gbuild_target_dir,
        };

        // Optimize the built wasm and return the artifact
        artifact.process(cargo_target_dir.join(format!(
            "wasm32-unknown-unknown/{}/{name}.wasm",
            if self.release { "release" } else { "debug" }
        )))?;
        Ok(artifact)
    }

    /// Process the cargo command.
    ///
    /// TODO: support workspace build.
    fn cargo(&self, target_dir: &Path) -> Result<()> {
        let mut kargo = CargoCommand::default();
        if self.release {
            kargo.set_profile("release".into());
        }
        kargo.set_manifest_path(self.manifest_path.clone());
        kargo.set_target_dir(target_dir.to_path_buf());
        kargo.set_features(&self.features);
        kargo.run()
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
        fs::create_dir_all(&self.root)
            .map_err(|e| anyhow!("Failed to create the artifact directory, {e}"))?;
        optimize::optimize_wasm(src, self.program.clone(), "4", true)?;
        let mut optimizer = Optimizer::new(self.program.clone())?;
        optimizer
            .insert_stack_end_export()
            .map_err(|e| anyhow!(e))?;
        optimizer.strip_custom_sections();
        fs::write(self.program.clone(), optimizer.optimize(OptType::Opt)?).map_err(Into::into)
    }
}
