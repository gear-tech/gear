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
use crate::app::App;
use anyhow::Result;
use clap::Parser;
use gsdk::ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32};

/// Transfer value.
#[derive(Clone, Debug, Parser)]
pub struct Transfer {
    /// Destination address.
    destination: String,

    /// Value to transfer.
    value: u128,
}

impl Transfer {
    pub async fn exec(self, app: &App) -> Result<()> {
        let api = app.signed_api().await?;

        let addr = AccountId32::from_ss58check(&self.destination)?;
        api.transfer_keep_alive(addr, self.value).await?;

        println!("Successfully transferred the value");

        Ok(())
    }
}
