// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Packages publisher

use crate::{
    handler, Manifest, Simulator, Workspace, PACKAGES, SAFE_DEPENDENCIES, STACKED_DEPENDENCIES,
};
use anyhow::{bail, Result};
use cargo_metadata::{Metadata, MetadataCommand};
use std::path::PathBuf;

/// crates-io packages publisher.
pub struct Publisher {
    metadata: Metadata,
    graph: Vec<Manifest>,
    index: Vec<&'static str>,
    workspace: Option<Workspace>,
    simulator: Option<Simulator>,
}

impl Publisher {
    /// Create a new publisher.
    pub fn new() -> Result<Self> {
        Self::with_simulation(false, None)
    }

    /// Create a new publisher with simulation.
    pub fn with_simulation(simulate: bool, registry_path: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            metadata: MetadataCommand::new().no_deps().exec()?,
            graph: vec![],
            index: [SAFE_DEPENDENCIES, STACKED_DEPENDENCIES, PACKAGES].concat(),
            workspace: None,
            simulator: simulate
                .then(|| Simulator::new(registry_path))
                .transpose()?,
        })
    }

    /// Build package graphs
    ///
    /// 1. Replace git dependencies to crates-io dependencies.
    /// 2. Rename version of all local packages
    /// 3. Patch dependencies if needed
    pub fn build(mut self, verify: bool, version: Option<String>) -> Result<Self> {
        let mut workspace = Workspace::lookup(version)?;
        let version = workspace.version()?;

        for name in self.index.iter() {
            let Some(pkg) = self.metadata.packages.iter().find(|pkg| pkg.name == *name) else {
                println!("Package {name}@{version} not found in cargo metadata!");
                continue;
            };

            if verify && crate::verify(name, &version, self.simulator.as_ref())? {
                println!("Package {name}@{version} already published!");
                continue;
            }

            self.graph.push(handler::patch(pkg)?);
        }

        workspace.complete(self.index.clone(), self.simulator.is_some())?;

        self.workspace = Some(workspace);

        self.patch()?;

        Ok(self)
    }

    /// Restore local files
    pub fn restore(&self) -> Result<()> {
        self.manifests()
            .map(|manifest| manifest.restore())
            .collect::<Result<Vec<_>>>()?;

        if let Some(workspace) = self.workspace.as_ref() {
            workspace.lock_file().restore()?;
        }

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.restore()?;
        }

        Ok(())
    }

    /// Patch local files
    fn patch(&self) -> Result<()> {
        self.manifests()
            .map(|manifest| manifest.patch())
            .collect::<Result<Vec<_>>>()?;

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.patch()?;
        }

        Ok(())
    }

    /// Returns an iterator of manifests that have been potentially patched
    fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.graph.iter().chain(self.workspace.as_deref())
    }

    /// Check the to-be-published packages
    ///
    /// TODO: Complete the check process (#3565)
    pub fn check(&self) -> Result<()> {
        let mut failed = Vec::new();
        for Manifest { path, name, .. } in self.graph.iter() {
            if !PACKAGES.contains(&name.as_str()) {
                continue;
            }

            println!("Checking {path:?}");
            let status = crate::check(&path.to_string_lossy())?;
            if !status.success() {
                failed.push(path);
            }
        }

        if !failed.is_empty() {
            bail!("Packages {failed:?} failed to pass the check ...");
        }

        // Post tests for gtest and gclient
        for (pkg, test) in [
            ("demo-syscall-error", "program_can_be_initialized"),
            ("gsdk", "timeout"),
        ] {
            if !crate::test(pkg, test)?.success() {
                bail!("{pkg}:{test} failed to pass the test ...");
            }
        }

        Ok(())
    }

    /// Publish packages
    pub fn publish(&self) -> Result<()> {
        for Manifest { path, .. } in self.graph.iter() {
            println!("Publishing {path:?}");
            let status = crate::publish(&path.to_string_lossy())?;
            if !status.success() {
                bail!("Failed to publish package {path:?} ...");
            }
        }

        Ok(())
    }
}
