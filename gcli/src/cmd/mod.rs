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

//! commands

pub mod claim;
pub mod config;
pub mod create;
pub mod info;
pub mod new;
pub mod program;
pub mod reply;
pub mod send;
pub mod transfer;
pub mod update;
pub mod upload;
pub mod wallet;

use std::time::Duration;

pub use self::{
    claim::Claim,
    config::{Config, ConfigSettings},
    create::Create,
    info::Info,
    new::New,
    program::Program,
    reply::Reply,
    send::Send,
    transfer::Transfer,
    update::Update,
    upload::Upload,
    wallet::Wallet,
};
use crate::App;
use anyhow::Result;
use clap::Parser;

/// All SubCommands of gear command line interface.
#[derive(Clone, Debug, Parser)]
pub enum Command {
    Claim(Claim),
    Create(Create),
    Info(Info),
    New(New),
    Config(Config),
    Program(Program),
    Reply(Reply),
    Send(Send),
    Upload(Upload),
    Transfer(Transfer),
    Update(Update),
    #[clap(subcommand)]
    Wallet(Wallet),
}

impl Command {
    /// Execute the command.
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        match self {
            Command::Config(config) => config.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Program(program) => program.exec(app).await?,
            Command::Update(update) => update.exec().await?,
            Command::Claim(claim) => claim.exec(app).await?,
            Command::Create(create) => create.exec(app).await?,
            Command::Info(info) => info.exec(app).await?,
            Command::Send(send) => send.exec(app).await?,
            Command::Upload(upload) => upload.exec(app).await?,
            Command::Transfer(transfer) => transfer.exec(app).await?,
            Command::Reply(reply) => reply.exec(app).await?,
            Command::Wallet(wallet) => wallet.run()?,
        }

        Ok(())
    }
}

/// Gear command-line interface.
#[derive(Debug, Parser)]
#[clap(author, version)]
#[command(name = "gcli")]
pub struct Opt {
    /// Commands.
    #[command(subcommand)]
    pub command: Command,
    /// Timeout for rpc requests.
    #[arg(short, long, default_value = "60000")]
    pub timeout: u64,
    /// Enable verbose logs.
    #[clap(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
    /// Gear node rpc endpoint.
    #[arg(short, long)]
    pub endpoint: Option<String>,
    /// Password of the signer account.
    #[arg(short, long)]
    pub passwd: Option<String>,
}

#[async_trait::async_trait]
impl App for Opt {
    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }

    fn verbose(&self) -> u8 {
        self.verbose
    }

    fn endpoint(&self) -> Option<String> {
        if self.endpoint.is_some() {
            return self.endpoint.clone();
        }

        ConfigSettings::read(None).ok().map(|c| c.url.to_string())
    }

    fn passwd(&self) -> Option<String> {
        self.passwd.clone()
    }

    async fn exec(&self) -> Result<()> {
        self.command.exec(self).await
    }
}

impl Opt {
    /// Run command sync.
    pub fn exec_sync(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(self.run())
    }
}
