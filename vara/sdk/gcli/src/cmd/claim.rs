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
