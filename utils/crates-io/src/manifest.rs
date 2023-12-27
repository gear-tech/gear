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
use toml_edit::{Document, InlineTable};

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

        self.rename_workspace()?;
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

    /// Rename worskapce manifest.
    fn rename_workspace(&mut self) -> Result<()> {
        self.ensure_workspace()?;

        let Some(deps) = self.manifest["workspace"]["dependencies"].as_table_like_mut() else {
            return Ok(());
        };

        for (name, dep) in deps.iter_mut() {
            let name = name.get();
            ["sp-", "frame-"]
                .iter()
                .any(|patt| name.starts_with(patt))
                .then(|| {
                    let Some(table) = dep.as_inline_table_mut() else {
                        return;
                    };
                    Self::rename_sub(name, table);
                });
        }

        Ok(())
    }

    /// Rename substrate dependencies.
    ///
    /// NOTE: The packages inside of this function are located at
    /// <https://github.com/gear-tech/substrate/tree/cl/1.0.3-crates-io>.
    fn rename_sub(name: &str, table: &mut InlineTable) {
        match name {
            // sp-allocator is outdated on crates.io, last
            // 3.0.0 forever, here we use gp-allocator instead.
            "sp-allocator" => {
                table.insert("version", "4.1.1".into());
                table.insert("package", "gp-allocator".into());
            }
            // Our sp-wasm-interface is different from the
            // original one.
            "sp-wasm-interface" => {
                table.insert("package", "gp-wasm-interface".into());
                table.insert("version", "7.0.1".into());
            }
            // Related to sp-wasm-interface.
            "sp-wasm-interface-common" => {
                table.insert("version", "7.0.1".into());
            }
            // Related to sp-wasm-interface.
            "sp-runtime-interface" => {
                table.insert("version", "7.0.3".into());
                table.insert("package", "gp-runtime-interface".into());
            }
            // The versions of these packages on crates.io are incorrect.
            "sp-arithmetic" | "sp-core" | "sp-rpc" | "sp-version" => {
                table.insert("version", "7.0.0".into());
            }
            // Filter out this package for local testing.
            "frame-support-test" => {
                return;
            }
            _ => {}
        }

        table.remove("branch");
        table.remove("git");
        table.remove("workspace");
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
