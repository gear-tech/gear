// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Test for infinity loop, that it can't exceed block production time.

use demo_custom::{InitMessage, WASM_BINARY};
use gsdk::{Result, events};
use parity_scale_codec::Encode;
use utils::dev_node;

mod utils;

#[tokio::test]
async fn backend_errors_handled_by_sandbox() -> Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice.
    let (_node, api) = dev_node().await;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let events = api.subscribe_all_events().await?;

    // Program initialization.
    let (message_id, _pid) = api
        .upload_program_bytes(
            WASM_BINARY,
            gear_utils::now_micros().to_le_bytes(),
            InitMessage::BackendError.encode(),
            gas_limit,
            0,
        )
        .await?
        .value;

    // Asserting successful initialization.
    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_success()
    );

    // Check no runtime panic occurred
    assert!(api.is_progressing().await?);

    Ok(())
}
