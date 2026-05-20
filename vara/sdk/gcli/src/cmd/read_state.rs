// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Command `read-state`.

use crate::{app::App, utils::HexBytes};
use anyhow::Result;
use clap::Parser;
use gear_core::ids::ActorId;
use gsdk::ext::subxt::utils::H256;

/// Ask program for its state.
#[derive(Clone, Debug, Parser)]
pub struct ReadState {
    /// Program ID.
    pid: ActorId,

    /// Payload for state request.
    #[arg(short, long, default_value = "0x")]
    payload: HexBytes,

    /// Hash of the block to read state at.
    #[arg(long)]
    at: Option<H256>,
}

impl ReadState {
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
