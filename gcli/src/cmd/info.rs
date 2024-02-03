// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use crate::{result::Result, App};
use clap::Parser;
use gclient::{
    ext::{
        sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT},
        sp_runtime::AccountId32,
    },
    metadata::runtime_types::gear_common::storage::primitives::Interval,
    GearApi,
};
use gear_core::message::UserStoredMessage;
use std::fmt;

#[derive(Clone, Debug, Parser)]
pub enum Action {
    /// Get balance info of the current account
    Balance,
    /// Get mailbox info of the current account
    Mailbox {
        /// The count of mails for fetching
        #[arg(default_value = "10", short, long)]
        count: u32,
    },
}

/// Get account info from ss58address.
#[derive(Clone, Debug, Parser)]
pub struct Info {
    /// Info of this address, if none, will use the logged in account.
    pub address: Option<String>,

    /// Info of balance, mailbox, etc.
    #[command(subcommand)]
    pub action: Action,
}

impl Info {
    /// execute command transfer
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        let signer = app.signer().await?;
        let mut address = self
            .address
            .clone()
            .unwrap_or_else(|| signer.account_id().to_ss58check());
        if address.starts_with("//") {
            address = Pair::from_string(&address, None)
                .expect("Parse development address failed")
                .public()
                .to_ss58check()
        }

        let acc = AccountId32::from_ss58check(&address)?;
        match self.action {
            Action::Balance => Self::balance(signer, acc).await,
            Action::Mailbox { count } => Self::mailbox(signer, acc, count).await,
        }
    }

    /// Get balance of address
    pub async fn balance(signer: GearApi, acc: AccountId32) -> Result<()> {
        let info = signer.free_balance(acc).await?;
        println!("Free balance: {info:#?}");
        Ok(())
    }

    /// Get mailbox of address
    pub async fn mailbox(signer: GearApi, acc: AccountId32, count: u32) -> Result<()> {
        let mails = signer.get_mailbox_account_messages(acc, count).await?;
        for t in mails.into_iter() {
            println!("{:#?}", Mail::from(t));
        }
        Ok(())
    }
}

/// Program mail for display
pub(crate) struct Mail {
    message: UserStoredMessage,
    interval: Interval<u32>,
}

impl From<(UserStoredMessage, Interval<u32>)> for Mail {
    fn from(t: (UserStoredMessage, Interval<u32>)) -> Self {
        Self {
            message: t.0,
            interval: t.1,
        }
    }
}

impl fmt::Debug for Mail {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Mail")
            .field(
                "id",
                &["0x", &hex::encode(self.message.id().into_bytes())].concat(),
            )
            .field(
                "source",
                &["0x", &hex::encode(self.message.source().into_bytes())].concat(),
            )
            .field(
                "destination",
                &["0x", &hex::encode(self.message.destination().into_bytes())].concat(),
            )
            .field(
                "payload",
                &["0x", &hex::encode(self.message.payload_bytes())].concat(),
            )
            .field("value", &self.message.value())
            .field("interval", &self.interval)
            .finish()
    }
}
