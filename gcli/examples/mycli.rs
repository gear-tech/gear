// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use color_eyre::Result;
use gcli::{App, Command, async_trait, clap::Parser};

/// My customized sub commands.
#[derive(Debug, Parser)]
pub enum SubCommand {
    /// GCli preset commands.
    #[clap(flatten)]
    GCliCommands(Command),
    /// My customized ping command.
    Ping,
}

/// My customized gcli.
#[derive(Debug, Parser)]
pub struct MyGCli {
    #[clap(subcommand)]
    command: SubCommand,
}

#[async_trait]
impl App for MyGCli {
    async fn exec(&self) -> Result<()> {
        match &self.command {
            SubCommand::GCliCommands(command) => command.exec(self).await,
            SubCommand::Ping => {
                println!("pong");
                Ok(())
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    MyGCli::parse().run().await
}
