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

//! Command `reply`
use crate::{App, result::Result, utils::Hex};
use clap::Parser;

/// Sends a reply message.
///
/// The origin must be Signed and the sender must have sufficient funds to pay
/// for `gas` and `value` (in case the latter is being transferred).
///
/// Parameters:
/// - `reply_to_id`: the original message id.
/// - `payload`: data expected by the original sender.
/// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
/// - `value`: balance to be transferred to the program once it's been created.
///
/// - `DispatchMessageEnqueued(H256)` when dispatch message is placed in the queue.
#[derive(Clone, Debug, Parser)]
pub struct Reply {
    /// Reply to
    reply_to_id: String,
    /// Reply payload
    #[arg(short, long, default_value = "0x")]
    payload: String,
    /// Reply gas limit
    ///
    /// Use estimated gas limit automatically if not set.
    #[arg(short, long)]
    gas_limit: Option<u64>,
    /// Reply value
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl Reply {
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        let signer = app.signer().await?;
        let reply_to_id = self.reply_to_id.to_hash()?;

        let gas_limit = if let Some(gas_limit) = self.gas_limit {
            gas_limit
        } else {
            signer
                .calculate_reply_gas(
                    None,
                    reply_to_id.into(),
                    self.payload.to_vec()?,
                    self.value,
                    false,
                )
                .await?
                .min_limit
        };

        signer
            .send_reply(
                reply_to_id.into(),
                self.payload.to_vec()?,
                gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
