// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! command `transfer`
use crate::app::App;
use anyhow::Result;
use clap::Parser;
use gear_core::ids::ActorId;

/// Transfer value.
#[derive(Clone, Debug, Parser)]
pub struct Transfer {
    /// Destination address, is SS58 or hex format.
    destination: ActorId,

    /// Value to transfer.
    value: u128,
}

impl Transfer {
    pub async fn exec(self, app: &App) -> Result<()> {
        let api = app.signed_api().await?;

        api.transfer_keep_alive(self.destination, self.value)
            .await?;

        println!("Successfully transferred the value");

        Ok(())
    }
}
