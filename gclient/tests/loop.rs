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

use gclient::{EventProcessor, GearApi, Result};

const PATH: &str = "../target/wasm32-unknown-unknown/release/demo_loop.opt.wasm";

#[tokio::test]
async fn inf_loop() -> Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice.
    let api = GearApi::dev().await?;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Program initialization.
    let (mid, pid, _) = api
        .upload_program_bytes_by_path(PATH, gclient::bytes_now(), "", gas_limit, 0)
        .await?;

    // Asserting successful initialization.
    assert!(listener.message_processed(mid).await?.succeed());

    // Sending message to trigger loop.
    let (mid, _) = api.send_message_bytes(pid, "", gas_limit, 0).await?;

    // Asserting message failure.
    assert!(listener.message_processed(mid).await?.failed());

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    Ok(())
}
