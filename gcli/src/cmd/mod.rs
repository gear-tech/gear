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
pub mod deploy;
pub mod info;
pub mod new;
pub mod read_state;
pub mod reply;
pub mod send;
pub mod transfer;
pub mod update;
pub mod upload_code;
pub mod wallet;

pub use self::{
    claim::Claim,
    config::{Config, ConfigSettings},
    deploy::Deploy,
    info::Info,
    new::New,
    read_state::ReadState,
    reply::Reply,
    send::Send,
    transfer::Transfer,
    update::Update,
    upload_code::UploadCode,
    wallet::Wallet,
};

use crate::app::App;
use anyhow::Result;
use clap::Parser;

/// All SubCommands of gear command line interface.
#[derive(Clone, Debug, Parser)]
pub enum Command {
    New(New),

    UploadCode(UploadCode),
    Deploy(Deploy),

    Info(Info),
    ReadState(ReadState),

    Send(Send),
    Reply(Reply),

    Transfer(Transfer),
    Claim(Claim),

    Config(Config),
    #[clap(subcommand)]
    Wallet(Wallet),

    Update(Update),
}

impl Command {
    /// Execute the command.
    pub async fn exec(self, app: &mut App) -> Result<()> {
        match self {
            Command::Config(config) => config.exec(app)?,
            Command::New(new) => new.exec().await?,
            Command::ReadState(program) => program.exec(app).await?,
            Command::Update(update) => update.exec().await?,
            Command::Claim(claim) => claim.exec(app).await?,
            Command::Deploy(create) => create.exec(app).await?,
            Command::Info(info) => info.exec(app).await?,
            Command::Send(send) => send.exec(app).await?,
            Command::UploadCode(upload) => upload.exec(app).await?,
            Command::Transfer(transfer) => transfer.exec(app).await?,
            Command::Reply(reply) => reply.exec(app).await?,
            Command::Wallet(wallet) => wallet.exec(app)?,
        }

        Ok(())
    }
}
