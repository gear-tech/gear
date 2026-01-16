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

//! Command `ask-state`.

use crate::{app::App, utils::HexBytes};
use anyhow::Result;
use clap::Parser;
use gear_core::ids::ActorId;
use gsdk::ext::subxt::utils::H256;

/// Ask program for its state.
#[derive(Clone, Debug, Parser)]
pub struct AskState {
    /// Program ID.
    pid: ActorId,

    /// Payload for state request.
    #[arg(short, long, default_value = "0x")]
    payload: HexBytes,

    /// Hash of the block to read state at.
    #[arg(long)]
    at: Option<H256>,
}

impl AskState {
    /// Run command program.
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let api = app.signed_api().await?;
        let state = api
            .read_state_bytes_at(self.pid, self.payload, self.at)
            .await?;
        println!("0x{}", hex::encode(state));
        Ok(())
    }
}
