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
use gear_wasm_builder::optimize::{self, OptType, Optimizer};
use std::{fs, path::PathBuf};

/// Gbuild artifact registry
///
/// This instance simply holds the paths of the built binaries
/// for re-using stuffs.
///
/// TODO: support workspace format, abstract instance for different programs (#3852)
pub struct Artifact {
    /// The directory path of the artifact.
    pub root: PathBuf,
    /// Program name of this artifact.
    pub name: String,
    /// The path to the built program.
    pub program: PathBuf,
}

impl Artifact {
    /// Create a new artifact registry.
    pub fn new(root: PathBuf, name: &str) -> Result<Self> {
        fs::create_dir_all(&root)
            .map_err(|e| anyhow!("Failed to create the artifact directory, {e}"))?;

        Ok(Self {
            program: root.join(format!("{name}.wasm")),
            name: name.replace('-', "_"),
            root,
        })
    }

    /// Build artifacts with optimization.
    pub fn process(&self, src: PathBuf) -> Result<()> {
        optimize::optimize_wasm(
            src.join(format!("{}.wasm", self.name)),
            self.program.clone(),
            "4",
            true,
        )?;
        let mut optimizer = Optimizer::new(self.program.clone())?;
        optimizer
            .insert_stack_end_export()
            .map_err(|e| anyhow!(e))?;
        optimizer.strip_custom_sections();
        fs::write(self.program.clone(), optimizer.optimize(OptType::Opt)?).map_err(Into::into)
    }
}
