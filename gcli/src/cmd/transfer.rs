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

//! command `transfer`
use crate::App;
use anyhow::Result;
use clap::Parser;
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

/// Transfer value.
///
/// # Note
///
/// Gear node is currently using the default properties of substrate for
/// [the staging testnet][0], and the decimals of 1 UNIT is 12 by default.
///
/// [0]: https://github.com/gear-tech/gear/blob/c01d0390cdf1031cb4eba940d0199d787ea480e0/node/src/chain_spec.rs#L218
#[derive(Clone, Debug, Parser)]
pub struct Transfer {
    /// Transfer to (ss58address).
    destination: String,
    /// Balance to transfer.
    value: u128,
}

impl Transfer {
    /// Execute command transfer.
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        let signer = app.signed().await?;
        let address = signer.account_id();

        println!("From: {}", address.to_ss58check());
        println!("To: {}", self.destination);
        println!("Value: {}", self.value);

        let addr = AccountId32::from_ss58check(&self.destination)?;
        signer.transfer_keep_alive(addr, self.value).await?;

        Ok(())
    }
}
