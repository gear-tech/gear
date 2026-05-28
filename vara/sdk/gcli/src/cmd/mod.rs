// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! commands

mod claim;
pub mod config;
mod create_program;
mod info;
mod new;
mod read_state;
mod reply;
mod send;
mod transfer;
#[cfg(feature = "self-update")]
mod update;
mod upload_code;
mod wallet;

#[cfg(feature = "self-update")]
use self::update::Update;
use self::{
    claim::Claim, config::Config, create_program::CreateProgram, info::Info, new::New,
    read_state::ReadState, reply::Reply, send::Send, transfer::Transfer, upload_code::UploadCode,
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
    CreateProgram(CreateProgram),

    Info(Info),
    ReadState(ReadState),

    Send(Send),
    Reply(Reply),

    Transfer(Transfer),
    Claim(Claim),

    Config(Config),
    #[clap(subcommand)]
    Wallet(Wallet),

    #[cfg(feature = "self-update")]
    Update(Update),
}

impl Command {
    /// Execute the command.
    pub async fn exec(self, app: &mut App) -> Result<()> {
        match self {
            Command::Config(config) => config.exec(app)?,
            Command::New(new) => new.exec().await?,
            Command::ReadState(program) => program.exec(app).await?,
            Command::Claim(claim) => claim.exec(app).await?,
            Command::CreateProgram(create) => create.exec(app).await?,
            Command::Info(info) => info.exec(app).await?,
            Command::Send(send) => send.exec(app).await?,
            Command::UploadCode(upload) => upload.exec(app).await?,
            Command::Transfer(transfer) => transfer.exec(app).await?,
            Command::Reply(reply) => reply.exec(app).await?,
            Command::Wallet(wallet) => wallet.exec(app)?,
            #[cfg(feature = "self-update")]
            Command::Update(update) => update.exec().await?,
        }

        Ok(())
    }
}
