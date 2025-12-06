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

//! Command `program`.
use crate::App;

use clap::Parser;
use color_eyre::Result;
use gear_core::ids::ActorId;
use gsdk::ext::subxt::utils::H256;

/// Read program state, etc.
#[derive(Clone, Debug, Parser)]
pub struct Program {
    /// Program id.
    pid: H256,
    /// The block hash for reading state.
    #[arg(long)]
    at: Option<H256>,
}

impl Program {
    /// Run command program.
    pub async fn exec(&self, app: &impl App) -> Result<()> {
        let api = app.signed().await?;
        let state = api
            .read_state_bytes_at(ActorId::new(self.pid.0), vec![], self.at)
            .await?;
        println!("0x{}", hex::encode(state));
        Ok(())
    }
}
