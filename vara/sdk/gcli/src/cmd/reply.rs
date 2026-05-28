// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Command `reply`
use crate::{app::App, utils::HexBytes};
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use gear_core::ids::MessageId;

/// Reply to a message.
#[derive(Clone, Debug, Parser)]
pub struct Reply {
    /// Message to reply to.
    message_id: MessageId,

    /// Reply payload, as hex string.
    #[arg(short, long, default_value = "0x")]
    payload: HexBytes,

    /// Operation gas limit.
    ///
    /// Defaults to the estimated gas limit
    /// required for the operation.
    #[arg(short, long)]
    gas_limit: Option<u64>,

    /// Value to send with the reply.
    #[arg(short, long, default_value = "0")]
    value: u128,
}

impl Reply {
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let api = app.signed_api().await?;

        let gas_limit = if let Some(gas_limit) = self.gas_limit {
            gas_limit
        } else {
            api.calculate_reply_gas(self.message_id, &self.payload, self.value, false)
                .await?
                .min_limit
        };

        let (message_id, _) = api
            .send_reply_bytes(
                self.message_id,
                self.payload.as_slice(),
                gas_limit,
                self.value,
            )
            .await?
            .value;

        println!("Successfully sent the reply");
        println!();
        println!("{} {}", "Message ID:".bold(), message_id);

        Ok(())
    }
}
