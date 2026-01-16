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

//! Command `send`
use crate::{app::App, utils::HexBytes};
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use gear_core::ids::ActorId;

/// Send a message.
#[derive(Clone, Debug, Parser)]
pub struct Send {
    /// Destination address.
    destination: ActorId,

    /// Message payload, as hex string.
    #[arg(short, long, default_value = "0x")]
    payload: HexBytes,

    /// Operation gas limit.
    ///
    /// Defaults to the estimated gas limit
    /// required for the operation.
    #[arg(short, long)]
    gas_limit: Option<u64>,

    /// Value to send with the message.
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl Send {
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let api = app.signed_api().await?;
        let gas_limit = if let Some(gas_limit) = self.gas_limit {
            gas_limit
        } else {
            api.calculate_handle_gas(self.destination, &self.payload, self.value, false)
                .await?
                .min_limit
        };

        let message_id = api
            .send_message_bytes(
                self.destination,
                self.payload.clone(),
                gas_limit,
                self.value,
            )
            .await?
            .value;

        println!("Successfully sent the message");
        println!();
        println!("{} {}", "Message ID:".bold(), message_id);
        Ok(())
    }
}
