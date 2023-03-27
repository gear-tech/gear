// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use demo_backend_error::WASM_BINARY;
use gclient::{EventProcessor, GearApi};

#[tokio::test]
async fn backend_errors_handled_by_sandbox() -> anyhow::Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice.
    let api = GearApi::dev_from_path("../target/release/gear").await?;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Program initialization.
    let (mid, _pid, _) = api
        .upload_program_bytes(
            WASM_BINARY,
            gclient::now_micros().to_le_bytes(),
            [],
            gas_limit,
            0,
        )
        .await?;

    // Asserting successful initialization.
    assert!(listener.message_processed(mid).await?.succeed());

    // Check no runtime panic occurred
    assert!(!api.queue_processing_stalled().await?);

    Ok(())
}
