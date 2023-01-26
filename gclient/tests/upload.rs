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

async fn upload_programs_and_check(
    api: &GearApi,
    codes: Vec<Vec<u8>>,
    timeout: Option<Duration>,
) -> Result<()> {
    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    let codes_len = codes.len();

    // Sending batch.
    let args: Vec<_> = codes
        .into_iter()
        .map(|code| {
            (
                code,
                gclient::now_in_micros().to_le_bytes(),
                "",
                gas_limit,
                0,
            )
        })
        .collect();
    let (ex_res, _) = if let Some(timeout) = timeout {
        async_std::future::timeout(timeout, api.upload_program_bytes_batch(args))
            .await
            .expect("Too long test upload time - something goes wrong.")?
    } else {
        api.upload_program_bytes_batch(args).await?
    };

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

    // Check no runtime panic occurred
    assert!(!api.queue_processing_stalled(&mut listener).await?);

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

    upload_programs_and_check(&api, codes, None).await?;

    Ok(())
}

#[tokio::test]
async fn alloc_zero_pages() -> Result<()> {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
    log::info!("Begin");
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
    upload_programs_and_check(&api, codes, Some(Duration::from_secs(5))).await
}
