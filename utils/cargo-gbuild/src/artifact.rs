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

use crate::metadata::Metadata;
use anyhow::{anyhow, Result};
use cargo_toml::Manifest;
use gear_wasm_builder::{
    optimize::{self, OptType, Optimizer},
    CargoCommand,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const ARTIFACT_DIR: &str = "gbuild";

/// Gbuild artifact registry
///
/// This instance simply holds the paths of the built binaries
/// for re-using stuffs.
pub struct Artifacts {
    /// cargo command
    kargo: CargoCommand,
    /// The path of the cargo wasm artifacts.
    pub root: PathBuf,
    /// artifact informations
    pub artifacts: Vec<Artifact>,
}

impl Artifacts {
    /// Create a new artifact registry.
    pub fn new(root: PathBuf, metadata: Metadata, kargo: CargoCommand) -> Result<Self> {
        fs::create_dir_all(&root)
            .map_err(|e| anyhow!("Failed to create the artifact directory, {e}"))?;

        Ok(Artifacts {
            root,
            kargo,
            artifacts: collect_crates(&metadata.gbuild.programs, OptType::Opt)?
                .into_iter()
                .chain(collect_crates(&metadata.gbuild.metas, OptType::Meta)?)
                .collect(),
        })
    }

    /// Process all artifacts
    pub fn process(&self) -> Result<()> {
        let gbuild = self
            .root
            .ancestors()
            .nth(2)
            .expect("Checked before passing in.")
            .join("gbuild");

        for artifact in self.artifacts.iter() {
            tracing::info!("Compile package {}", artifact.name);
            let mut kargo = self.kargo.clone();
            kargo.set_manifest_path(artifact.manifest.clone());
            kargo.run();

            tracing::info!("Optimizing package {}", artifact.name);
            artifact.optimize(&self.root, &gbuild)?;
        }

        Ok(())
    }
}

/// Program atrifact
pub struct Artifact {
    /// The original manifest path.
    pub manifest: PathBuf,
    /// Optimization type
    pub opt: OptType,
    /// Program name of this artifact.
    pub name: String,
}

impl Artifact {
    fn file_name(&self) -> String {
        self.name.clone()
            + if self.opt.is_meta() {
                ".meta.wasm"
            } else {
                ".wasm"
            }
    }

    /// Fetch and optimize artifact
    pub fn optimize(&self, src: &Path, root: &Path) -> Result<()> {
        let name = self.file_name();
        let output = root.join(&name);

        optimize::optimize_wasm(src.join(&name), output.clone(), "4", true)?;
        let mut optimizer = Optimizer::new(output.clone())?;
        if !self.opt.is_meta() {
            optimizer
                .insert_stack_end_export()
                .map_err(|e| anyhow!(e))?;
            optimizer.strip_custom_sections();
        }

        fs::write(output, optimizer.optimize(self.opt)?).map_err(Into::into)
    }
}

/// Collection crate manifests from the provided glob patterns.
fn collect_crates(patterns: &[String], opt: OptType) -> Result<Vec<Artifact>> {
    let mut crates: Vec<PathBuf> = Default::default();
    for p in patterns {
        crates.append(
            &mut glob::glob(p)?
                .filter_map(|p| {
                    p.ok().and_then(|p| {
                        let manifest = p.join("Cargo.toml");
                        if manifest.exists() {
                            Some(manifest)
                        } else {
                            tracing::warn!("Invalid manifest: {manifest:?}");
                            None
                        }
                    })
                })
                .collect(),
        );
    }

    crates
        .into_iter()
        .map(|manifest| -> Result<Artifact> {
            Ok(Artifact {
                name: Manifest::from_path(&manifest).map(|m| m.package().name().into())?,
                opt,
                manifest,
            })
        })
        .collect()
}
