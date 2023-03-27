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

use gclient::{EventProcessor, GearApi};

const PATHS: [&str; 2] = [
    "../target/wat-examples/wrong_load.wasm",
    "../target/wat-examples/inf_recursion.wasm",
];

async fn upload_programs_and_check(
    api: &GearApi,
    codes: Vec<Vec<u8>>,
    timeout: Option<Duration>,
) -> anyhow::Result<()> {
    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    let codes_len = codes.len();

    // Sending batch.
    let args: Vec<_> = codes
        .into_iter()
        .map(|code| (code, gclient::now_micros().to_le_bytes(), "", gas_limit, 0))
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
    assert!(!api.queue_processing_stalled().await?);

    Ok(())
}

#[tokio::test]
async fn harmless_upload() -> anyhow::Result<()> {
    let mut codes = vec![];
    for path in &PATHS {
        codes.push(gclient::code_from_os(path)?);
    }

    // Creating gear api.
    //
    // By default, login as Alice, than re-login as Bob.
    let api = GearApi::dev_from_path("../target/release/gear")
        .await?
        .with("//Bob")?;

    upload_programs_and_check(&api, codes, None).await?;

    Ok(())
}

#[tokio::test]
async fn alloc_zero_pages() -> anyhow::Result<()> {
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
    let api = GearApi::dev_from_path("../target/release/gear")
        .await?
        .with("//Bob")?;
    let codes = vec![wat::parse_str(wat_code).unwrap()];
    upload_programs_and_check(&api, codes, Some(Duration::from_secs(5))).await
}

#[tokio::test]
async fn get_mailbox() -> anyhow::Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice, than re-login as Bob.
    let api = GearApi::dev_from_path("../target/release/gear")
        .await?
        .with("//Bob")?;

    // Subscribe to events
    let mut listener = api.subscribe().await?;

    // Check that blocks are still running
    assert!(listener.blocks_running().await?);

    let wat_code = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_reply_push" (func $reply_push (param i32 i32 i32)))
        (import "env" "gr_reply_commit" (func $reply_commit (param i32 i32 i32)))
        (export "init" (func $init))
        (export "handle" (func $handle))
        (func $init)
        (func $handle
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))
            (call $reply_push (i32.const 0) (i32.const 0xfa00) (i32.const 100))

            ;; sending commit
            (call $reply_commit (i32.const 10) (i32.const 0) (i32.const 200))
        )
        (data (i32.const 0) "PONG")
    )"#;

    let code = wat::parse_str(wat_code).unwrap();

    // Calculate gas amount needed for initialization
    let gas_info = api
        .calculate_upload_gas(None, code.clone(), vec![], 0, true)
        .await?;

    // Upload and init the program
    let (message_id, program_id, _hash) = api
        .upload_program_bytes(
            code,
            gclient::now_micros().to_le_bytes(),
            vec![],
            gas_info.min_limit,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Calculate gas amount needed for handling the message
    let gas_info = api
        .calculate_handle_gas(None, program_id, vec![], 0, true)
        .await?;

    let messages = vec![(program_id, vec![], gas_info.min_limit * 10, 0); 5];

    let (messages, _hash) = api.send_message_bytes_batch(messages).await?;

    let (message_id, _hash) = messages.last().unwrap().as_ref().unwrap();

    assert!(listener.message_processed(*message_id).await?.succeed());

    let mailbox = api.get_mailbox_messages(15).await?;

    // Check that all messages is in mailbox
    assert_eq!(mailbox.len(), 5);

    for msg in mailbox {
        assert_eq!(msg.0.payload().len(), 1000 * 1024); // 1MB payload
        assert!(msg.0.payload().starts_with(b"PONG"));
    }

    Ok(())
}
