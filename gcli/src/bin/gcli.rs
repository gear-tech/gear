// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use clap::Parser;
use gcli::{cmd::Command, App};
use gsdk::signer::Signer;

/// Gear command-line interface.
#[derive(Debug, Parser)]
#[clap(author, version)]
#[command(name = "gcli")]
pub struct Opt {
    /// Commands.
    #[command(subcommand)]
    pub command: Command,
    /// How many times we'll retry when RPC requests failed.
    #[arg(short, long, default_value = "5")]
    pub retry: u16,
    /// Enable verbose logs.
    #[clap(short, long, action = clap::ArgAction::Count)]
    pub verbose: u16,
    /// Gear node rpc endpoint.
    #[arg(short, long)]
    pub endpoint: Option<String>,
    /// Password of the signer account.
    #[arg(short, long)]
    pub passwd: Option<String>,
}

#[async_trait::async_trait]
impl App for Opt {
    fn retry(&self) -> u16 {
        self.retry
    }

    fn verbose(&self) -> u16 {
        self.verbose
    }

    fn endpoint(&self) -> Option<String> {
        self.endpoint.clone()
    }

    fn passwd(&self) -> Option<String> {
        self.passwd.clone()
    }

    async fn exec(self, signer: Signer) -> anyhow::Result<()> {
        self.command.exec(signer).await
    }
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    Opt::run().await
}
