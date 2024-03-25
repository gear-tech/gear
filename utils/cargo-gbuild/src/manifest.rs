// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::{fs, path::PathBuf};

/// Cargo manifest abstraction.
#[derive(Debug, Deserialize)]
struct Manifest {
    /// The build section in the cargo manifest.
    pub build: Option<Build>,
}

/// The build section in the cargo manifest.
#[derive(Debug, Deserialize)]
struct Build {
    /// The target directory of the cargo project.
    pub target_dir: Option<String>,
}

/// Parse the target directory from the manifest.
pub fn parse_target(manifest: &PathBuf) -> Result<PathBuf> {
    Ok(toml::from_str::<Manifest>(&fs::read_to_string(manifest)?)?
        .build
        .and_then(|b| b.target_dir)
        .map(PathBuf::from)
        .unwrap_or(
            manifest
                .parent()
                .ok_or_else(|| anyhow!("Could not parse target directory from {manifest:?}"))?
                .join("target/gbuild"),
        ))
}
