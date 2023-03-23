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

//! command `info`
use crate::result::{Error, Result};
use clap::Parser;
use gsdk::{
    ext::{
        sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT},
        sp_runtime::AccountId32,
    },
    metadata::runtime_types::{
        gear_common::storage::primitives::Interval,
        gear_core::message::{
            common::{MessageDetails, ReplyDetails, SignalDetails},
            stored::StoredMessage,
        },
    },
    signer::Signer,
};
use std::fmt;

#[derive(Debug, Parser)]
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
#[derive(Debug, Parser)]
pub struct Info {
    /// Info of this address, if none, will use the logged in account.
    pub address: Option<String>,

    /// Info of balance, mailbox, etc.
    #[command(subcommand)]
    pub action: Action,
}

impl Info {
    /// execute command transfer
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let mut address = self.address.clone().unwrap_or_else(|| signer.address());
        if address.starts_with("//") {
            address = Pair::from_string(&address, None)
                .expect("Parse development address failed")
                .public()
                .to_ss58check()
        }

        match self.action {
            Action::Balance => Self::balance(signer, &address).await,
            Action::Mailbox { count } => Self::mailbox(signer, &address, count).await,
        }
    }

    /// Get balance of address
    pub async fn balance(signer: Signer, address: &str) -> Result<()> {
        let info = signer.api().info(address).await?;

        println!("{info:#?}");

        Ok(())
    }

    /// Get mailbox of address
    pub async fn mailbox(signer: Signer, address: &str, count: u32) -> Result<()> {
        let mails = signer
            .api()
            .mailbox(
                Some(AccountId32::from_ss58check(address).map_err(|_| Error::InvalidPublic)?),
                count,
            )
            .await?;

        for t in mails.into_iter() {
            println!("{:#?}", Mail::from(t));
        }
        Ok(())
    }
}

struct Mail {
    message: StoredMessage,
    interval: Interval<u32>,
}

impl From<(StoredMessage, Interval<u32>)> for Mail {
    fn from(t: (StoredMessage, Interval<u32>)) -> Self {
        Self {
            message: t.0,
            interval: t.1,
        }
    }
}

impl fmt::Debug for Mail {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Mail")
            .field("id", &["0x", &hex::encode(self.message.id.0)].concat())
            .field(
                "source",
                &["0x", &hex::encode(self.message.source.0)].concat(),
            )
            .field(
                "destination",
                &["0x", &hex::encode(self.message.destination.0)].concat(),
            )
            .field(
                "payload",
                &["0x", &hex::encode(&self.message.payload.0)].concat(),
            )
            .field("value", &self.message.value)
            .field(
                "details",
                &self.message.details.as_ref().map(DebugMessageDestination),
            )
            .field("interval", &self.interval)
            .finish()
    }
}

struct DebugMessageDestination<'d>(pub &'d MessageDetails);

impl<'d> fmt::Debug for DebugMessageDestination<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut d = fmt.debug_tuple("MessageDetails");
        match self.0 {
            MessageDetails::Reply(reply) => d.field(&DebugReplyDetails(reply)),
            MessageDetails::Signal(signal) => d.field(&DebugSignalDestination(signal)),
        };
        d.finish()
    }
}

struct DebugReplyDetails<'d>(pub &'d ReplyDetails);

impl<'d> fmt::Debug for DebugReplyDetails<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ReplyDetails")
            .field("reply_to", &hex::encode(self.0.reply_to.0))
            .field("status_code", &self.0.status_code.to_string())
            .finish()
    }
}

struct DebugSignalDestination<'d>(pub &'d SignalDetails);

impl<'d> fmt::Debug for DebugSignalDestination<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SignalDetails")
            .field("from", &hex::encode(self.0.from.0))
            .field("status_code", &self.0.status_code.to_string())
            .finish()
    }
}
