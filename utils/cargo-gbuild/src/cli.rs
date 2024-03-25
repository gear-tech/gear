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

use crate::manifest;
use anyhow::Result;
use ccli::{clap, Parser};
use std::path::PathBuf;

/// Command `gbuild` as cargo extension.
#[derive(Parser)]
pub struct GBuild {
    /// The path to the program manifest
    #[clap(short, long, default_value = "Cargo.toml")]
    pub manifest_path: PathBuf,

    /// Space or comma separated list of features to activate
    #[clap(short, long)]
    pub features: Vec<String>,

    /// Directory for all generated artifacts
    #[clap(short, long)]
    pub target_dir: Option<PathBuf>,
}

impl GBuild {
    /// Build program
    pub fn build(&self) -> Result<()> {
        let _target = self
            .target_dir
            .clone()
            .unwrap_or(manifest::parse_target(&self.manifest_path)?);

        // TODO: inheirt the logging format of cargo.
        //
        // for example:
        //
        // Compiling program-name v0.0.1 ( /path/to/the/program )
        tracing::info!("Program artifacts have been output in {_target:?} !");
        Ok(())
    }
}
