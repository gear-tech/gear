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

use anyhow::Result;
use cargo_toml::{Manifest, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Cargo manifest with path
pub struct ManifestWithPath {
    /// Crate name
    pub name: String,
    /// Cargo manifest
    pub manifest: Manifest,
    /// Path of the manifest
    pub path: PathBuf,
}

impl ManifestWithPath {
    /// Get the workspace manifest
    pub fn workspace() -> Result<Self> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|workspace_dir| workspace_dir.join("Cargo.toml"))
            .ok_or_else(|| anyhow::anyhow!("Could not find workspace manifest"))?
            .canonicalize()?;

        Ok(Self {
            name: "__gear_workspace".into(),
            manifest: Manifest::from_path(&path)?,
            path,
        })
    }

    /// Complete the manifest of the specified crate from
    /// the current manifest
    pub fn manifest(&self, path: impl AsRef<Path>) -> Result<Self> {
        let mut manifest = Manifest::<Value>::from_slice_with_metadata(&fs::read(&path)?)?;
        manifest
            .complete_from_path_and_workspace(path.as_ref(), Some((&self.manifest, &self.path)))?;

        Ok(Self {
            name: manifest.package.clone().map(|p| p.name).unwrap_or_default(),
            manifest,
            path: path.as_ref().to_path_buf(),
        })
    }
}
