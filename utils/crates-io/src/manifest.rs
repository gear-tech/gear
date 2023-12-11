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

//! Manifest utils for crates-io-manager

use anyhow::{anyhow, Result};
use cargo_metadata::Package;
use std::{fs, path::PathBuf};
use toml_edit::Document;

use crate::version;

const WORKSPACE_NAME: &str = "__gear_workspace";
const INHERITS: [&str; 6] = [
    "version",
    "authors",
    "edition",
    "license",
    "homepage",
    "repository",
];

/// Cargo manifest with path
pub struct Manifest {
    /// Crate name
    pub name: String,
    /// Cargo manifest
    pub manifest: Document,
    /// Path of the manifest
    pub path: PathBuf,
}

impl Manifest {
    /// Get the workspace manifest
    pub fn workspace() -> Result<Self> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|workspace_dir| workspace_dir.join("Cargo.toml"))
            .ok_or_else(|| anyhow::anyhow!("Could not find workspace manifest"))?
            .canonicalize()?;

        Ok(Self {
            name: WORKSPACE_NAME.to_string(),
            manifest: fs::read_to_string(&path)?.parse()?,
            path,
        })
    }

    /// Set version for the workspace.
    pub fn with_version(mut self, version: Option<String>) -> Result<Self> {
        self.ensure_workspace()?;

        let version = if let Some(version) = version {
            version
        } else {
            let cur = self.manifest["workspace"]["package"]
                .get_mut("version")
                .ok_or_else(|| {
                    anyhow!(
                        "Could not find version in workspace manifest: {}",
                        self.path.display()
                    )
                })?
                .to_string();

            cur + "-" + &version::hash()?
        };

        self.manifest["workspace"]["package"]["version"] = toml_edit::value(version);

        Ok(self)
    }

    /// Complete the manifest of the specified crate from
    /// the workspace manifest
    pub fn manifest(&self, pkg: &Package) -> Result<Self> {
        self.ensure_workspace()?;

        // Inherit metadata from workspace
        let mut manifest: Document = fs::read_to_string(&pkg.manifest_path)?.parse()?;
        for inherit in INHERITS {
            manifest["package"][inherit] = self.manifest["workspace"]["package"][inherit].clone();
        }

        // Complete documentation as from <https://docs.rs>
        let name = pkg.name.clone();
        manifest["package"]["documentation"] = toml_edit::value(format!("https://docs.rs/{name}"));

        Ok(Self {
            name,
            manifest,
            path: pkg.manifest_path.clone().into(),
        })
    }

    /// Ensure the current function is called on the workspace manifest
    fn ensure_workspace(&self) -> Result<()> {
        if self.name != WORKSPACE_NAME {
            return Err(anyhow!(
                "This method can only be called on the workspace manifest"
            ));
        }

        Ok(())
    }
}
