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

use crate::utils;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Gbuild metadata
#[derive(Serialize, Deserialize)]
pub struct GbuildMetadata {
    /// Gear programs in the workspace.
    programs: Vec<String>,
    /// Gear program metas in the workspace.
    metas: Vec<String>,
}

impl GbuildMetadata {
    /// Collect all gear programs
    pub fn programs(&self) -> Result<Vec<PathBuf>> {
        utils::collect_crates(&self.programs)
    }

    /// Collect all gear metas
    pub fn metas(&self) -> Result<Vec<PathBuf>> {
        utils::collect_crates(&self.metas)
    }
}

/// Cargo gbuild metadata
#[derive(Serialize, Deserialize)]
pub struct Metadata {
    /// Gbuild metadata,
    pub gbuild: GbuildMetadata,
}
