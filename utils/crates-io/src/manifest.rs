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

    /// Complete the manifest of the specified crate from
    /// the workspace manifest
    pub fn manifest(&self, pkg: &Package) -> Result<Self> {
        self.ensure_workspace()?;

        // Complete documentation as from <https://docs.rs>
        let mut manifest: Document = fs::read_to_string(&pkg.manifest_path)?.parse()?;
        let name = pkg.name.clone();
        manifest["package"]["documentation"] = toml_edit::value(format!("https://docs.rs/{name}"));

        Ok(Self {
            name,
            manifest,
            path: pkg.manifest_path.clone().into(),
        })
    }

    /// complete the versions of the specified crates
    pub fn complete_versions(&mut self, index: &[&str]) -> Result<()> {
        self.ensure_workspace()?;

        let version = self.manifest["workspace"]["package"]["version"]
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

        self.rename_deps()?;
        Ok(())
    }

    /// Set version for the workspace.
    pub fn with_version(mut self, version: Option<String>) -> Result<Self> {
        self.ensure_workspace()?;

        let version = if let Some(version) = version {
            version
        } else {
            self.version()? + "-" + &version::hash()?
        };

        self.manifest["workspace"]["package"]["version"] = toml_edit::value(version);

        Ok(self)
    }

    /// Get version from the current manifest.
    pub fn version(&self) -> Result<String> {
        self.ensure_workspace()?;

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

    /// Write manifest to disk.
    pub fn write(&self) -> Result<()> {
        fs::write(&self.path, self.manifest.to_string()).map_err(Into::into)
    }

    /// Rename dependencies
    fn rename_deps(&mut self) -> Result<()> {
        self.ensure_workspace()?;

        let Some(deps) = self.manifest["workspace"]["dependencies"].as_table_like_mut() else {
            return Ok(());
        };

        for (name, dep) in deps.iter_mut() {
            let name = name.get();
            if !name.starts_with("sp-") {
                continue;
            }

            // Format dotted values into inline table.
            if let Some(table) = dep.as_table_mut() {
                table.remove("branch");
                table.remove("git");
                table.remove("workspace");

                if name == "sp-arithmetic" {
                    // NOTE: the required version of sp-arithmetic is 6.0.0 in
                    // git repo, but 7.0.0 in crates.io, so we need to fix it.
                    table.insert("version", toml_edit::value("7.0.0"));
                }

                // Force the dep to be inline table in case of losing
                // documentation.
                let mut inline = table.clone().into_inline_table();
                inline.fmt();
                *dep = toml_edit::value(inline);
            };
        }

        Ok(())
    }

    /// Ensure the current function is called on the workspace manifest
    ///
    /// TODO: remove this interface after #3565
    fn ensure_workspace(&self) -> Result<()> {
        if self.name != WORKSPACE_NAME {
            return Err(anyhow!(
                "This method can only be called on the workspace manifest"
            ));
        }

        Ok(())
    }
}
