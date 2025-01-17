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

use anyhow::Result;
use clap::{CommandFactory, Parser};
use gring::cmd::Command;
use tracing_subscriber::filter::EnvFilter;

/// Gear keyring.
#[derive(Parser)]
pub struct Opt {
    /// The verbosity level.
    #[arg(global = true, short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Sub commands.
    #[command(subcommand)]
    pub command: Command,
}

impl Opt {
    /// Run the CLI with logger.
    pub fn start() -> Result<()> {
        let app = Self::parse();
        let name = Self::command().get_name().to_string();
        let env = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(match app.verbose {
            0 => format!("{name}=info"),
            1 => format!("{name}=debug"),
            2 => "debug".into(),
            _ => "trace".into(),
        }));

        tracing_subscriber::fmt().with_env_filter(env).init();
        app.command.run()
    }
}

fn main() -> anyhow::Result<()> {
    Opt::start()
}
