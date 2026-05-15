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
