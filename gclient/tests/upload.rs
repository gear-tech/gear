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

use std::time::Duration;

use gclient::{EventProcessor, GearApi, Result};

const PATHS: [&str; 2] = [
    "../target/wat-examples/wrong_load.wasm",
    "../target/wat-examples/inf_recursion.wasm",
];

async fn upload_programs_and_check(api: &GearApi, codes: Vec<Vec<u8>>) -> Result<()> {
    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit().await?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    let codes_len = codes.len();

    // Sending batch.
    let args: Vec<_> = codes
        .into_iter()
        .map(|code| (code, gclient::bytes_now(), "", gas_limit, 0))
        .collect();
    let (ex_res, _) = api.upload_program_bytes_batch(args).await?;

    // Ids of initial messages.
    let mids: Vec<_> = ex_res
        .into_iter()
        .filter_map(|v| v.ok().map(|(mid, _pid)| mid))
        .collect();

    // Checking that all upload program calls succeed in batch.
    assert_eq!(codes_len, mids.len());

    // Checking that all batch got processed.
    assert_eq!(
        codes_len,
        listener.message_processed_batch(mids).await?.len(),
    );

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    Ok(())
}

#[tokio::test]
async fn harmless_upload() -> Result<()> {
    let mut codes = vec![];
    for path in &PATHS {
        codes.push(gclient::code_from_os(path)?);
    }

    // Creating gear api.
    //
    // By default, login as Alice, than re-login as Bob.
    let api = GearApi::dev().await?.with("//Bob")?;

    upload_programs_and_check(&api, codes).await?;

    Ok(())
}

#[test]
fn alloc_zero_pages() {
    #[tokio::main()]
    async fn start_tokio() -> Result<()> {
        let wat_code = r#"
        (module
            (import "env" "memory" (memory 0))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (export "init" (func $init))
            (func $init
                i32.const 0
                call $alloc
                drop
            )
        )"#;
        let api = GearApi::dev().await?.with("//Bob")?;
        let codes = vec![wat::parse_str(wat_code).unwrap()];
        upload_programs_and_check(&api, codes).await
    }

    let t = std::thread::spawn(start_tokio);

    // Wait 5 seconds - if node correctly handle program upload, then thread would finish execution.
    std::thread::sleep(Duration::from_secs(5));

    if !t.is_finished() {
        panic!("Alloc zero pages test running too long");
    } else {
        t.join().unwrap().unwrap();
    }
}
