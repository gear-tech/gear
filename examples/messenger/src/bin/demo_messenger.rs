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
#![cfg(feature = "gcli")]

use gcli::{
    anyhow, async_trait,
    clap::{self, Parser},
    cmd::Upload,
    color_eyre, tokio, App,
};

#[derive(Debug, Parser)]
pub enum Command {
    Upload(Upload),
}

#[derive(Debug, Parser)]
pub struct Messager {
    #[clap(subcommand)]
    command: Command,
}

#[async_trait]
impl App for Messager {
    async fn exec(&self) -> anyhow::Result<()> {
        let lookup = gcli::lookup!();

        let Command::Upload(upload) = &self.command;
        upload
            .clone_with_code_overridden(lookup.opt)
            .exec(self)
            .await
            .map_err(Into::into)
    }
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    Messager::parse().run().await
}
