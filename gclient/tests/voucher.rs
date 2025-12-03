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

//! Test for infinity loop, that it can't exceed block production time.

use demo_custom::{InitMessage, WASM_BINARY};
use gclient::{EventProcessor, GearApi};
use gear_core::ids::ActorId;
use parity_scale_codec::Encode;

#[tokio::test]
async fn voucher_issue_and_upload_code_and_send_message() -> anyhow::Result<()> {
    // Creating gear api.
    let api = GearApi::dev_from_path("../target/release/gear").await?;
    let actor_id =
        ActorId::try_from(api.account_id().encode().as_ref()).expect("failed to create actor id");
    let voucher_initial_balance = 100_000_000_000_000;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Taking account balance.
    let _balance = api.api().total_balance(api.account_id()).await?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Issue voucher
    let (voucher_id, ..) = api
        .issue_voucher(actor_id, voucher_initial_balance, None, true, 100)
        .await?;

    // Upload code with voucher
    let (code_id, _) = api
        .upload_code_with_voucher(voucher_id.clone(), WASM_BINARY)
        .await?;

    // Create program
    let payload = InitMessage::Capacitor("15".to_string()).encode();
    let (message_id, program_id, ..) = api
        .signer()
        .calls()
        .create_program_bytes(code_id, vec![], payload, gas_limit, 0)
        .await?;

    // Asserting message succeed
    assert!(listener.message_processed(message_id).await?.succeed());

    // Send message with voucher
    let payload = b"10".to_vec();
    let (message_id, ..) = api
        .send_message_bytes_with_voucher(
            voucher_id.clone(),
            program_id,
            payload,
            gas_limit,
            0,
            true,
        )
        .await?;

    // Asserting message succeed
    assert!(listener.message_processed(message_id).await?.succeed());

    // Decline voucher
    let (_voucher_id, ..) = api.decline_voucher_with_voucher(voucher_id.clone()).await?;

    Ok(())
}
