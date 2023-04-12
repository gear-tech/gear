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

//! Command `send`
use crate::{result::Result, utils::Hex};
use clap::Parser;
use gsdk::signer::Signer;

/// Sends a message to a program or to another account.
///
/// The origin must be Signed and the sender must have sufficient funds to pay
/// for `gas` and `value` (in case the latter is being transferred).
///
/// To avoid an undefined behavior a check is made that the destination address
/// is not a program in uninitialized state. If the opposite holds true,
/// the message is not enqueued for processing.
///
/// Parameters:
/// - `destination`: the message destination.
/// - `payload`: in case of a program destination, parameters of the `handle` function.
/// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
/// - `value`: balance to be transferred to the program once it's been created.
///
/// Emits the following events:
/// - `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue.
#[derive(Parser, Debug)]
pub struct Send {
    /// Send to
    pub destination: String,
    /// Send payload
    #[arg(short, long, default_value = "0x")]
    pub payload: String,
    /// Send gas limit
    #[arg(short, long, default_value = "0")]
    pub gas_limit: u64,
    /// Send value
    #[arg(short, long, default_value = "0")]
    pub value: u128,
}

impl Send {
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        signer
            .send_message(
                self.destination.to_hash()?.into(),
                self.payload.to_vec()?,
                self.gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
