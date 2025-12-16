// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use anyhow::{Context, Result};
use clap::Parser;
use commands::Command;
use params::Params;
use std::path::PathBuf;

mod commands;
mod params;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Path to the TOML config file. If not provided, the default path "./.ethexe.toml" is used. To disable parsing of the config file, use "none".
    #[arg(long)]
    pub cfg: Option<String>,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Default path to the TOML config file.
    pub const DEFAULT_PARAMS_PATH: &str = "./.ethexe.toml";

    /// Run the CLI.
    pub fn run(self) -> Result<()> {
        let params = self
            .file_params()
            .with_context(|| "failed to read params from file")?
            .unwrap_or_default();

        self.command.run(params)
    }

    fn file_params(&self) -> Result<Option<Params>> {
        Ok(match &self.cfg {
            Some(path_str) if path_str == "none" => None,
            Some(path) => {
                let path = PathBuf::from(path);

                println!("📄 Using custom params file: {}", path.display());

                Some(Params::from_file(path)?)
            }
            None => {
                let default_cfg_path = PathBuf::from(Self::DEFAULT_PARAMS_PATH);

                if default_cfg_path.exists() {
                    println!(
                        "📄 Using default params file: {}",
                        default_cfg_path.display()
                    );

                    Some(Params::from_file(default_cfg_path)?)
                } else {
                    None
                }
            }
        })
    }
}
