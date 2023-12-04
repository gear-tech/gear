// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::{rename, ManifestWithPath, PACKAGES, SAFE_DEPENDENCIES, STACKED_DEPENDENCIES};
use anyhow::Result;
use cargo_metadata::{Metadata, MetadataCommand};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
};

/// crates-io packages publisher.
pub struct Publisher {
    metadata: Metadata,
    graph: BTreeMap<Option<usize>, ManifestWithPath>,
    index: HashMap<String, usize>,
}

impl Publisher {
    /// Create a new publisher.
    pub fn new() -> Result<Self> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let graph = BTreeMap::new();
        let index = HashMap::<String, usize>::from_iter(
            [
                SAFE_DEPENDENCIES.to_vec(),
                STACKED_DEPENDENCIES.into(),
                PACKAGES.into(),
            ]
            .concat()
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p.into(), i)),
        );

        Ok(Self {
            metadata,
            graph,
            index,
        })
    }

    /// Build package graphs
    ///
    /// 1. Replace git dependencies to crates-io dependencies.
    /// 2. Rename version of all local packages
    /// 3. Patch dependencies if needed
    pub fn build(mut self) -> Result<Self> {
        let workspace = ManifestWithPath::workspace()?;
        for p in &self.metadata.packages {
            if !self.index.contains_key(&p.name) {
                continue;
            }

            let version = p.version.to_string();
            println!("Verifying {}@{} ...", &p.name, &version);
            if crate::verify(&p.name, &version)? {
                println!("Package {}@{} already published .", &p.name, &version);
                continue;
            }

            let mut manifest = workspace.manifest(&p.manifest_path)?;
            rename::package(p, &mut manifest.manifest)?;
            rename::deps(&mut manifest.manifest, self.index.keys().collect(), version)?;

            self.graph
                .insert(self.index.get(&p.name).cloned(), manifest);
        }

        Ok(self)
    }

    /// Check the to-be-published packages
    ///
    /// TODO: Complete the check process (#3565)
    pub fn check(&self) -> Result<()> {
        self.flush()?;

        let mut failed = Vec::new();
        for ManifestWithPath { path, name, .. } in self.graph.values() {
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
            panic!("Packages {failed:?} failed to pass the check ...");
        }

        Ok(())
    }

    /// Publish packages
    pub fn publish(&self) -> Result<()> {
        self.flush()?;

        for ManifestWithPath { path, .. } in self.graph.values() {
            println!("Publishing {path:?}");
            let status = crate::publish(&path.to_string_lossy())?;
            if !status.success() {
                panic!("Failed to publish package {path:?} ...");
            }
        }

        Ok(())
    }

    /// Flush new manifests to disk
    fn flush(&self) -> Result<()> {
        for ManifestWithPath { path, manifest, .. } in self.graph.values() {
            fs::write(path, toml::to_string_pretty(&manifest)?)?;
        }

        Ok(())
    }
}
