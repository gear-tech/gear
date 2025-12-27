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

//! command `info`
use crate::{app::App, utils::HexBytes};
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use gsdk::{
    Api,
    ext::{
        sp_core::{Pair as PairT, crypto::Ss58Codec, sr25519::Pair},
        sp_runtime::AccountId32,
    },
};

/// Get account info.
#[derive(Clone, Debug, Parser)]
pub struct Info {
    /// Account address, defaults to the current account.
    address: Option<String>,

    #[command(subcommand)]
    action: Action,
}

#[derive(Clone, Debug, Parser)]
pub enum Action {
    /// Get account balance.
    Balance,
    /// List messages in the mailbox.
    Mailbox {
        /// Limit number of fetched messages.
        #[arg(default_value = "10", short, long)]
        count: usize,
    },
}

impl Info {
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let mut address = self
            .address
            .clone()
            .map_or_else(|| app.ss58_address(), Ok)?;

        let api = app.api().await?;

        if address.starts_with("//") {
            address = Pair::from_string(&address, None)
                .expect("Parse development address failed")
                .public()
                .to_ss58check()
        }

        let acc = AccountId32::from_ss58check(&address)?;
        match self.action {
            Action::Balance => Self::print_balance(&api, acc).await,
            Action::Mailbox { count } => Self::print_mailbox(&api, acc, count).await,
        }
    }

    /// Prints the account balance.
    async fn print_balance(api: &Api, acc: AccountId32) -> Result<()> {
        let balance = api.free_balance(acc).await?;
        println!("{} {}", "Free balance:".bold(), balance);
        Ok(())
    }

    /// Prints the account mailbox.
    async fn print_mailbox(api: &Api, acc: AccountId32, count: usize) -> Result<()> {
        let mails = api.mailbox_messages(acc, count).await?;
        if mails.is_empty() {
            println!("{}", "Mailbox is empty".dimmed());
        }

        for (message, interval) in mails {
            println!("{} {}", "id:".bold(), message.id());
            println!("{} {}", "source:".bold(), message.source());
            println!("{} {}", "destination:".bold(), message.destination());
            println!(
                "{} {}",
                "payload:".bold(),
                HexBytes::from(message.payload_bytes().to_vec())
            );
            println!("{} {}", "value:".bold(), message.value());
            println!(
                "{} {}..{}",
                "interval:".bold(),
                interval.start,
                interval.finish
            );
            println!("{}", "---".dimmed());
        }
        Ok(())
    }
}
