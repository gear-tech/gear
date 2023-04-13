// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use crate::{keystore, result::Result};
use clap::Parser;
use env_logger::{Builder, Env};
use gsdk::Api;
use log::LevelFilter;

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

/// Commands of cli `gear`
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

/// gear command-line tools
///    ___     ___     _       ___  
///   / __|   / __|   | |     |_ _|
///  | (_ |  | (__    | |__    | |  
///   \___|   \___|   |____|  |___|
/// _|"""""|_|"""""|_|"""""|_|"""""|
/// "`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'
#[derive(Debug, Parser)]
#[clap(author, version, verbatim_doc_comment)]
#[command(name = "gcli")]
pub struct Opt {
    /// Commands.
    #[command(subcommand)]
    pub command: Command,
    /// How many times we'll retry when RPC requests failed.
    #[arg(short, long, default_value = "5")]
    pub retry: u16,
    /// Enable verbose logs.
    #[arg(short, long)]
    pub verbose: bool,
    /// Gear node rpc endpoint.
    #[arg(short, long)]
    pub endpoint: Option<String>,
    /// Password of the signer account.
    #[arg(short, long)]
    pub passwd: Option<String>,
}

impl Opt {
    /// setup logs
    fn setup_logs(&self) -> Result<()> {
        let mut builder = if self.verbose {
            Builder::from_env(Env::default().default_filter_or("gcli=debug"))
        } else {
            match &self.command {
                Command::Claim(_)
                | Command::Create(_)
                | Command::Reply(_)
                | Command::Send(_)
                | Command::Upload(_)
                | Command::Transfer(_) => {
                    let mut builder = Builder::from_env(Env::default().default_filter_or("info"));
                    builder
                        .format_target(false)
                        .format_module_path(false)
                        .format_timestamp(None)
                        .filter_level(LevelFilter::Info);

                    builder
                }
                _ => Builder::from_default_env(),
            }
        };

        builder.try_init()?;
        Ok(())
    }

    /// run program
    pub async fn run() -> Result<()> {
        let opt = Opt::parse();

        opt.setup_logs()?;
        opt.exec().await?;
        Ok(())
    }

    /// Create api client from endpoint
    async fn api(&self) -> Result<Api> {
        Api::new(self.endpoint.as_deref()).await.map_err(Into::into)
    }

    /// Execute command sync
    pub fn exec_sync(&self) -> color_eyre::Result<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(self.exec()).map_err(Into::into)
    }

    /// Execute command.
    pub async fn exec(&self) -> Result<()> {
        match &self.command {
            Command::Key(key) => key.exec(self.passwd.as_deref())?,
            Command::Login(login) => login.exec()?,
            Command::New(new) => new.exec().await?,
            Command::Program(program) => program.exec(self.api().await?).await?,
            Command::Update(update) => update.exec().await?,
            sub => {
                let api = self.api().await?;
                let pair = if let Ok(s) = keystore::cache(self.passwd.as_deref()) {
                    s
                } else {
                    keystore::keyring(self.passwd.as_deref())?
                };
                let signer = (api, pair).into();

                match sub {
                    Command::Claim(claim) => claim.exec(signer).await?,
                    Command::Create(create) => create.exec(signer).await?,
                    Command::Info(info) => info.exec(signer).await?,
                    Command::Send(send) => send.exec(signer).await?,
                    Command::Upload(upload) => upload.exec(signer).await?,
                    Command::Transfer(transfer) => transfer.exec(signer).await?,
                    Command::Reply(reply) => reply.exec(signer).await?,
                    _ => unreachable!("Already matched"),
                }
            }
        }

        Ok(())
    }
}
