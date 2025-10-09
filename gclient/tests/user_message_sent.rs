// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use demo_ping::WASM_BINARY;
use futures::StreamExt;
use gclient::{GearApi, UserMessageSentFilter};
use gear_core::ids::ActorId;
use std::convert::TryFrom;

const GEAR_PATH: &str = "../target/release/gear";

/// Running this test requires gear node to be built in advance.
#[tokio::test]
async fn subscribe_user_messages_receives_reply() -> anyhow::Result<()> {
    let api = GearApi::dev_from_path(GEAR_PATH).await?;

    let destination =
        ActorId::try_from(api.account_id().as_ref()).expect("account id must be a valid ActorId");

    let mut subscription = api
        .subscribe_user_message_sent(
            UserMessageSentFilter::new()
                .with_destination(destination)
                .with_payload_prefix(b"PONG"),
        )
        .await?;

    let gas_limit = api.block_gas_limit()?;
    let salt = gclient::now_micros().to_le_bytes();

    // Upload `demo_ping` with payload that triggers the reply to the user.
    api.upload_program_bytes(WASM_BINARY, salt, b"PING".to_vec(), gas_limit, 0)
        .await?;

    let mut received = None;
    for _ in 0..10 {
        match subscription.next().await {
            Some(Ok(event)) if event.destination == destination => {
                if event.payload == b"PONG" {
                    received = Some(event);
                    break;
                }
            }
            Some(Ok(_)) => continue,
            Some(Err(err)) => return Err(err.into()),
            None => break,
        }
    }

    let event = received.expect("expected user message reply");
    assert_eq!(event.payload, b"PONG");

    Ok(())
}
