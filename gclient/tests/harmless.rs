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

//! Test for harmful demos, checking their init can't brake the chain.

use gclient::{EventProcessor, GearApi, Result};

const PATHS: [&str; 2] = [
    "../target/wat-examples/wrong_load.wasm",
    "../target/wat-examples/inf_recursion.wasm",
];

#[tokio::test]
async fn harmless_upload() -> Result<()> {
    // Creating gear api.
    let api = GearApi::dev().await?;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit().await?;

    // Creating batch arguments.
    let mut args = vec![];

    for path in &PATHS {
        args.push((
            gclient::code_from_os(path)?,
            gclient::bytes_now(),
            "",
            gas_limit,
            0,
        ));
    }

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Sending batch.
    let (ex_res, _) = api.upload_program_bytes_batch(args).await?;

    // Ids of initial messages.
    let mids: Vec<_> = ex_res
        .into_iter()
        .filter_map(|v| v.ok().map(|(mid, _pid)| mid))
        .collect();

    // Checking that all upload program calls succeed in batch.
    assert_eq!(PATHS.len(), mids.len());

    // Checking that all batch got processed.
    assert_eq!(
        listener.message_processed_batch(mids).await?.len(),
        PATHS.len()
    );

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    Ok(())
}
