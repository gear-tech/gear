// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! This crate provides the main CLI interface.

use crate::{
    app::{App, Opts},
    cmd::Command,
};
use anyhow::Result;
use clap::Parser;

/// Interact with Gear API via node RPC.
#[derive(Debug, Clone, Parser)]
#[clap(author, version)]
pub struct Cli {
    #[command(flatten)]
    opts: Opts,

    /// Command to run.
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        App::new(self.opts).run(self.command).await
    }

    pub fn run_blocking(self) -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(self.run())
    }
}
