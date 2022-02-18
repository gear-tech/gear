// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use anyhow::{Context, Result};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use std::path::Path;

use crate::builder_error::BuilderError;

/// Helper to get a crate info exctracted from the `Cargo.toml`.
#[derive(Debug, Default)]
pub struct CrateInfo {
    /// Original name of the crate.
    pub name: String,
    /// Crate name converted to the snake case.
    pub snake_case_name: String,
    /// Crate version.
    pub version: String,
}

impl CrateInfo {
    /// Create a new `CrateInfo` from a path to the `Cargo.toml`.
    pub fn from_manifest(manifest_path: &Path) -> Result<Self> {
        anyhow::ensure!(
            manifest_path.exists(),
            BuilderError::InvalidManifestPath(manifest_path.to_path_buf())
        );

        let mut meta_cmd = MetadataCommand::new();
        let metadata = meta_cmd
            .manifest_path(manifest_path)
            .exec()
            .context("unable to invoke `cargo metadata`")?;

        let root_package =
            Self::root_package(&metadata).ok_or(BuilderError::RootPackageNotFound)?;
        let name = root_package.name.clone();
        let snake_case_name = name.replace('-', "_");
        let version = root_package.version.to_string();

        Ok(Self {
            name,
            snake_case_name,
            version,
        })
    }

    fn root_package(metadata: &Metadata) -> Option<&Package> {
        let root_id = metadata.resolve.as_ref()?.root.as_ref()?;
        metadata
            .packages
            .iter()
            .find(|package| package.id == *root_id)
    }
}
