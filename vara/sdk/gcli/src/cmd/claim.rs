// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Command `claim`
use crate::app::App;
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use gear_core::ids::MessageId;

/// Claim value from message in the mailbox.
#[derive(Clone, Debug, Parser)]
pub struct Claim {
    /// Message to claim value from.
    message_id: MessageId,
}

impl Claim {
    pub async fn exec(self, app: &mut App) -> Result<()> {
        let value = app
            .signed_api()
            .await?
            .claim_value(self.message_id)
            .await?
            .value;
        println!("Successfully claimed value of {}", value.to_string().blue());
        Ok(())
    }
}
