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

//! Manifest utils for crates-io-manager

use crate::{handler, version};
use anyhow::{anyhow, Result};
use cargo_metadata::Package;
use std::{
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
};
use toml_edit::DocumentMut;

const WORKSPACE_NAME: &str = "__gear_workspace";

/// Workspace instance, which is a wrapper of [`Manifest`].
pub struct Workspace(Manifest);

impl Workspace {
    /// Get the workspace manifest with version overridden.
    pub fn lookup(version: Option<String>) -> Result<Self> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|workspace_dir| workspace_dir.join("Cargo.toml"))
            .ok_or_else(|| anyhow::anyhow!("Could not find workspace manifest"))?
            .canonicalize()?;

        let mut workspace: Self = Manifest {
            name: WORKSPACE_NAME.to_string(),
            manifest: fs::read_to_string(&path)?.parse()?,
            path,
        }
        .into();

        // NOTE: renaming version here is required because it could
        // be easy to publish incorrect version to crates.io by mistake
        // in testing.
        {
            let version = if let Some(version) = version {
                version
            } else {
                workspace.version()? + "-" + &version::hash()?
            };

            workspace.manifest["workspace"]["package"]["version"] = toml_edit::value(version);
        }

        Ok(workspace)
    }

    /// complete the versions of the specified crates
    pub fn complete(&mut self, mut index: Vec<&str>) -> Result<()> {
        handler::patch_alias(&mut index);

        let version = self.0.manifest["workspace"]["package"]["version"]
            .clone()
            .as_str()
            .ok_or_else(|| anyhow!("Could not find version in workspace manifest"))?
            .to_string();

        let Some(deps) = self.manifest["workspace"]["dependencies"].as_table_mut() else {
            return Err(anyhow!(
                "Failed to parse dependencies from workspace {}",
                self.path.display()
            ));
        };

        for (key, dep) in deps.iter_mut() {
            let name = key.get();
            if !index.contains(&name) {
                continue;
            }

            dep["version"] = toml_edit::value(version.clone());
        }

        self.rename()?;
        Ok(())
    }

    /// Get version from the current manifest.
    pub fn version(&self) -> Result<String> {
        Ok(self.manifest["workspace"]["package"]["version"]
            .as_str()
            .ok_or_else(|| {
                anyhow!(
                    "Could not find version in workspace manifest: {}",
                    self.path.display()
                )
            })?
            .to_string())
    }

    /// Rename worskapce manifest.
    fn rename(&mut self) -> Result<()> {
        let Some(deps) = self.manifest["workspace"]["dependencies"].as_table_like_mut() else {
            return Ok(());
        };

        for (name, dep) in deps.iter_mut() {
            let name = name.get();
            let Some(table) = dep.as_inline_table_mut() else {
                continue;
            };

            handler::patch_workspace(name, table);
        }

        Ok(())
    }
}

impl From<Manifest> for Workspace {
    fn from(manifest: Manifest) -> Self {
        Self(manifest)
    }
}

impl Deref for Workspace {
    type Target = Manifest;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Cargo manifest with path
pub struct Manifest {
    /// Crate name
    pub name: String,
    /// Cargo manifest
    pub manifest: DocumentMut,
    /// Path of the manifest
    pub path: PathBuf,
}

impl Manifest {
    /// Complete the manifest of the specified crate from
    /// the workspace manifest
    pub fn new(pkg: &Package) -> Result<Self> {
        // Complete documentation as from <https://docs.rs>
        let mut manifest: DocumentMut = fs::read_to_string(&pkg.manifest_path)?.parse()?;
        let name = pkg.name.clone();
        manifest["package"]["documentation"] = toml_edit::value(format!("https://docs.rs/{name}"));

        Ok(Self {
            name,
            manifest,
            path: pkg.manifest_path.clone().into(),
        })
    }

    /// Write manifest to disk.
    pub fn write(&self) -> Result<()> {
        fs::write(&self.path, self.manifest.to_string()).map_err(Into::into)
    }
}

impl From<Workspace> for Manifest {
    fn from(workspace: Workspace) -> Self {
        workspace.0
    }
}
