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

//! commands
use crate::App;
use clap::Parser;

pub mod claim;
pub mod create;
pub mod info;
pub mod key;
pub mod login;
pub mod new;
pub mod program;
pub mod reply;
pub mod send;
pub mod transfer;
pub mod update;
pub mod upload;

/// All SubCommands of gear command line interface.
#[derive(Debug, Parser)]
pub enum Command {
    Claim(claim::Claim),
    Create(create::Create),
    Info(info::Info),
    Key(key::Key),
    Login(login::Login),
    New(new::New),
    #[clap(subcommand)]
    Program(program::Program),
    Reply(reply::Reply),
    Send(send::Send),
    Upload(upload::Upload),
    Transfer(transfer::Transfer),
    Update(update::Update),
}

impl Command {
    /// Execute the command.
    pub async fn exec(&self, app: &impl App) -> anyhow::Result<()> {
        match self {
            Command::Key(key) => key.exec()?,
            Command::Login(login) => login.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Program(program) => program.exec(app).await?,
            Command::Update(update) => update.exec().await?,
            Command::Claim(claim) => claim.exec(app.signer().await?).await?,
            Command::Create(create) => create.exec(app.signer().await?).await?,
            Command::Info(info) => info.exec(app.signer().await?).await?,
            Command::Send(send) => send.exec(app.signer().await?).await?,
            Command::Upload(upload) => upload.exec(app.signer().await?).await?,
            Command::Transfer(transfer) => transfer.exec(app.signer().await?).await?,
            Command::Reply(reply) => reply.exec(app.signer().await?).await?,
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
    fn timeout(&self) -> u64 {
        self.timeout
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

    async fn exec(&self) -> anyhow::Result<()> {
        self.command.exec(self).await
    }
}

impl Opt {
    /// Run command sync.
    pub fn exec_sync(&self) -> color_eyre::Result<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(self.run()).map_err(Into::into)
    }
}
