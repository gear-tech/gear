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
