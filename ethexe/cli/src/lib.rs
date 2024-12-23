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

use anyhow::{Context, Ok, Result};
use clap::Parser;
use commands::Command;
use params::Params;
use std::path::PathBuf;

mod commands;
mod params;

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long)]
    cfg: Option<String>,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub const DEFAULT_PARAMS_PATH: &str = "./.ethexe.toml";

    pub async fn run(self) -> Result<()> {
        let params = self
            .file_params()
            .with_context(|| "failed to read params from file")?
            .unwrap_or_default();

        self.command.run(params).await
    }

    fn file_params(&self) -> Result<Option<Params>> {
        Ok(match &self.cfg {
            Some(ref path_str) if path_str == "none" => None,
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
