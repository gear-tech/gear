// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{CARGO_REGISTRY_NAME, handler, version};
use anyhow::{Result, anyhow};
use cargo_metadata::Package;
use std::{
    env, fs,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item};

const WORKSPACE_NAME: &str = "__gear_workspace";

/// Workspace instance, which is a wrapper of [`Manifest`].
pub struct Workspace {
    manifest: Manifest,
    lock_file: LockFile,
}

impl Workspace {
    /// Get the workspace manifest with version overridden.
    pub fn lookup(version: Option<String>) -> Result<Self> {
        let path = Self::resolve_path("Cargo.toml")?;
        let original_manifest: DocumentMut = fs::read_to_string(&path)?.parse()?;
        let mutable_manifest = original_manifest.clone();

        let lock_file_path = Self::resolve_path("Cargo.lock")?;
        let content = fs::read_to_string(&lock_file_path)?;

        let mut workspace = Self {
            manifest: Manifest {
                name: WORKSPACE_NAME.to_string(),
                original_manifest,
                mutable_manifest,
                path,
                is_published: true,
                is_actualized: true,
            },
            lock_file: LockFile {
                content,
                path: lock_file_path,
            },
        };

        // NOTE: renaming version here is required because it could
        // be easy to publish incorrect version to crates.io by mistake
        // in testing.
        {
            let version = if let Some(version) = version {
                version
            } else {
                workspace.version()? + "-" + &version::hash()? + "commit"
            };

            workspace.mutable_manifest["workspace"]["package"]["version"] =
                toml_edit::value(version);
        }

        workspace.mutable_manifest["workspace"]["dependencies"]["gstd"]["features"] = Item::None;

        Ok(workspace)
    }

    /// Resolve path to file in workspace.
    pub fn resolve_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .ancestors()
            .nth(2)
            .map(|workspace_dir| workspace_dir.join(path.as_ref()))
            .ok_or_else(|| anyhow!("Could not find workspace manifest"))?
            .canonicalize()?;
        Ok(path)
    }

    /// Complete the versions of the specified crates.
    pub fn complete(&mut self, mut index: Vec<&str>, simulate: bool) -> Result<()> {
        handler::patch_alias(&mut index);

        let version = self.mutable_manifest["workspace"]["package"]["version"]
            .clone()
            .as_str()
            .ok_or_else(|| anyhow!("Could not find version in workspace manifest"))?
            .to_string();

        let Some(deps) = self.mutable_manifest["workspace"]["dependencies"].as_table_like_mut()
        else {
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

            dep["version"] = toml_edit::value(format!("={version}"));

            if simulate {
                dep["registry"] = toml_edit::value(CARGO_REGISTRY_NAME);
            }
        }

        self.rename()?;
        Ok(())
    }

    /// Get version from the current manifest.
    pub fn version(&self) -> Result<String> {
        Ok(self.mutable_manifest["workspace"]["package"]["version"]
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
        let Some(deps) = self.mutable_manifest["workspace"]["dependencies"].as_table_like_mut()
        else {
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

    /// Returns Cargo lock file
    pub fn lock_file(&self) -> &LockFile {
        &self.lock_file
    }
}

impl Deref for Workspace {
    type Target = Manifest;

    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

impl DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manifest
    }
}

/// Cargo manifest with path
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Crate name
    pub name: String,
    /// Original cargo manifest
    pub original_manifest: DocumentMut,
    /// Cargo manifest
    pub mutable_manifest: DocumentMut,
    /// Path of the manifest
    pub path: PathBuf,
    /// Whether the crate is published
    pub is_published: bool,
    /// Whether the current version is published
    pub is_actualized: bool,
}

impl Manifest {
    /// Complete the manifest of the specified crate from
    /// the workspace manifest
    pub fn new(pkg: &Package, is_published: bool, is_actualized: bool) -> Result<Self> {
        let original_manifest: DocumentMut = fs::read_to_string(&pkg.manifest_path)?.parse()?;
        let mut mutable_manifest = original_manifest.clone();

        // Complete documentation as from <https://docs.rs>
        let name = pkg.name.clone().into_inner();
        mutable_manifest["package"]["documentation"] =
            toml_edit::value(format!("https://docs.rs/{name}"));

        Ok(Self {
            name,
            original_manifest,
            mutable_manifest,
            path: pkg.manifest_path.clone().into(),
            is_published,
            is_actualized,
        })
    }

    /// Restore manifest
    pub fn restore(&self) -> Result<()> {
        fs::write(&self.path, self.original_manifest.to_string()).map_err(Into::into)
    }

    /// Patch manifest
    pub fn patch(&self) -> Result<()> {
        fs::write(&self.path, self.mutable_manifest.to_string()).map_err(Into::into)
    }
}

/// Cargo lock file with path
#[derive(Debug, Clone)]
pub struct LockFile {
    content: String,
    path: PathBuf,
}

impl LockFile {
    /// Restore lock file
    pub fn restore(&self) -> Result<()> {
        fs::write(&self.path, &self.content).map_err(Into::into)
    }
}
