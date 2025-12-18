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

//! command `create`
use crate::{App, utils::Hex};
use anyhow::Result;
use clap::Parser;

/// Deploy program to gear node
#[derive(Clone, Debug, Parser)]
pub struct Create {
    /// gear program code id
    code_id: String,
    /// gear program salt ( hex encoding )
    #[arg(short, long, default_value = "0x")]
    salt: String,
    /// gear program init payload ( hex encoding )
    #[arg(short, long, default_value = "0x")]
    init_payload: String,
    /// gear program gas limit
    ///
    /// Use estimated gas limit automatically if not set.
    #[arg(short, long)]
    gas_limit: Option<u64>,
    /// gear program balance
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl Create {
    /// Exec command submit
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        let code_id = self.code_id.to_hash()?.into();
        let payload = self.init_payload.to_vec()?;
        let signer = app.signed().await?;

        // estimate gas
        let gas_limit = if let Some(gas_limit) = self.gas_limit {
            gas_limit
        } else {
            signer
                .calculate_create_gas(code_id, payload.clone(), self.value, false)
                .await?
                .min_limit
        };

        // create program
        signer
            .create_program(code_id, self.salt.to_vec()?, payload, gas_limit, self.value)
            .await?;

        Ok(())
    }
}
