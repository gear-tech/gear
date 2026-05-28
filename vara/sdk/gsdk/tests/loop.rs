// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Test for infinity loop, that it can't exceed block production time.

use demo_constructor::{Calls, Scheme, WASM_BINARY};
use gsdk::events;
use parity_scale_codec::Encode;
use utils::dev_node;

mod utils;

#[tokio::test]
async fn inf_loop() -> gsdk::Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice.
    let (_node, api) = dev_node().await;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Taking account balance.
    let _balance = api.total_balance().await?;

    // Subscribing for events.
    let events = api.subscribe_all_events().await?;

    // Program initialization with infinite loop inside.
    let (message_id, _pid) = api
        .upload_program_bytes(
            WASM_BINARY,
            gear_utils::now_micros().to_le_bytes(),
            Scheme::direct(Calls::builder().infinite_loop()).encode(),
            gas_limit,
            0,
        )
        .await?
        .value;

    // Asserting message failure.
    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_failed()
    );

    // Checking that blocks still running.
    assert!(api.is_progressing().await?);

    Ok(())
}
