// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use anyhow::{Result, anyhow};
use cargo_toml::Manifest;
use colored::Colorize;
use gear_wasm_optimizer::{self as optimize, CargoCommand, Optimizer};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Gbuild artifact registry
///
/// This instance simply holds the paths of the built binaries
/// for re-using stuffs.
pub struct Artifacts {
    /// cargo command
    kargo: CargoCommand,
    /// The path of the cargo wasm artifacts.
    pub source: PathBuf,
    /// The path of the gbuild wasm artifacts.
    pub root: PathBuf,
    /// artifact information
    pub artifacts: Vec<Artifact>,
}

impl Artifacts {
    /// Create a new artifact registry.
    pub fn new(
        root: PathBuf,
        source: PathBuf,
        metadata: Metadata,
        kargo: CargoCommand,
    ) -> Result<Self> {
        fs::create_dir_all(&root)
            .map_err(|e| anyhow!("Failed to create the artifact directory, {e}"))?;

        let cwd = env::current_dir()?;
        env::set_current_dir(&metadata.workspace_root);

        // Collect all possible packages from metadata
        let mut artifacts: Vec<Artifact> = collect_crates(&cwd, &metadata.gbuild.programs)?
            .into_iter()
            .collect();

        // If not using workspace build, filter out the matched package
        // from metas and programs.
        if !metadata.workspace {
            let current: Vec<Artifact> = artifacts
                .iter()
                .filter(|a| a.manifest.eq(&metadata.manifest))
                .cloned()
                .collect();

            if current.is_empty() {
                let manifest = Manifest::from_path(&metadata.manifest)?;
                if manifest.package.is_some() {
                    artifacts = vec![Artifact {
                        manifest: metadata.manifest,
                        name: manifest.package().name.clone(),
                    }];
                }
            } else {
                artifacts = current;
            }
        }

        env::set_current_dir(cwd)?;
        Ok(Artifacts {
            source,
            root,
            kargo,
            artifacts,
        })
    }

    /// Process all artifacts
    pub fn process(&self) -> Result<()> {
        let all = self.artifacts.len();
        for (idx, artifact) in self.artifacts.iter().enumerate() {
            tracing::info!(
                "[{}/{all}] Compiling package {} ...",
                idx + 1,
                artifact.name.bold()
            );
            let mut kargo = self.kargo.clone();
            kargo.set_manifest_path(artifact.manifest.clone());
            kargo.run()?;
            artifact.optimize(&self.source, &self.root)?;
        }

        tracing::info!("Finished ({})", self.root.to_string_lossy());
        Ok(())
    }

    /// List all artifacts
    pub fn list(&self) -> Vec<PathBuf> {
        self.artifacts
            .iter()
            .map(|a| self.root.join(a.names().1))
            .collect()
    }
}

/// Program artifact
#[derive(Clone, Debug)]
pub struct Artifact {
    /// The original manifest path.
    pub manifest: PathBuf,
    /// Program name of this artifact.
    pub name: String,
}

impl Artifact {
    /// Returns the input and the output name of the program
    fn names(&self) -> (String, String) {
        let name = self.name.replace('-', "_");
        let input = name.clone() + ".wasm";
        let output = input.clone();
        (input, output)
    }

    /// Fetch and optimize artifact
    pub fn optimize(&self, src: &Path, root: &Path) -> Result<()> {
        let (input, output) = self.names();
        let output = root.join(output);

        let mut optimizer = Optimizer::new(&src.join(input))?;
        optimizer
            .insert_stack_end_export()
            .map_err(|e| anyhow!("{e}"));
        optimizer.strip_custom_sections();
        optimizer.strip_exports();
        optimizer.flush_to_file(&output);

        optimize::optimize_wasm(&output, &output, "4", true)?;

        Ok(())
    }
}

/// Collection crate manifests from the provided glob patterns.
fn collect_crates(cwd: &Path, patterns: &[String]) -> Result<Vec<Artifact>> {
    let cwd = env::current_dir()?;
    let mut crates: Vec<PathBuf> = Default::default();
    for p in patterns {
        crates.append(
            &mut glob::glob(p)?
                .filter_map(|p| {
                    p.ok().and_then(|p| {
                        tracing::trace!("checking {p:?}");
                        let manifest = cwd.join(p.join("Cargo.toml"));
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
            let mut toml = Manifest::from_slice(&fs::read(&manifest)?)?;
            let mut cdylib = false;
            if let Some(lib) = &toml.lib {
                cdylib = lib.crate_type.contains(&"cdylib".to_string());
            }

            // Specifying `--crate-type` in rustc flags doesn't work
            // for our case since crates like gstd, gmeta don't have
            // `cdylib` specified and it would slow down compilation
            // time if adding `cdylib` in their [lib].
            //
            // So here we simply panic if users don't have `cdylib`
            // in their manifest, otherwise using shadow manifest
            // could be a dirty solution.
            //
            // see: https://github.com/rust-lang/cargo/issues/11232
            if !cdylib {
                eprint!(
                    "{}: could not find `cdylib` in [lib.crate-type] from {}",
                    "error".bold().red(),
                    manifest.display()
                );
                std::process::exit(1);
            }

            Ok(Artifact {
                name: toml.package().name().into(),
                manifest,
            })
        })
        .collect()
}
