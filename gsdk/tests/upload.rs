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

//! Test for harmful demos, checking their init can't brake the chain.

use std::{pin::pin, time::Duration};

use demo_wat::WatExample;
use gear_core::{code::MAX_WASM_PAGES_AMOUNT, pages::WasmPage};
use gsdk::{AccountKeyring, Result, RuntimeError, SignedApi, events, gear::gear};
use utils::dev_node;

mod utils;

async fn upload_programs_and_check(
    api: &SignedApi,
    codes: Vec<Vec<u8>>,
    timeout: Option<Duration>,
) -> Result<()> {
    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let events = api.subscribe_all_events().await?;

    let codes_len = codes.len();

    // Sending batch.
    let args: Vec<_> = codes
        .into_iter()
        .map(|code| {
            (
                code,
                gear_utils::now_micros().to_le_bytes(),
                "",
                gas_limit,
                0,
            )
        })
        .collect();
    let ex_res = if let Some(timeout) = timeout {
        tokio::time::timeout(timeout, api.upload_program_bytes_batch(args))
            .await
            .expect("Too long test upload time - something goes wrong")?
    } else {
        api.upload_program_bytes_batch(args).await?
    }
    .value;

    // Ids of initial messages.
    let message_ids = ex_res
        .into_iter()
        .map(|res| res.map(|(message_id, _)| message_id))
        .collect::<Result<Vec<_>>>()?;

    // Checking that all upload program calls succeed in batch.
    assert_eq!(codes_len, message_ids.len());

    // Checking that all batch got processed.
    events::message_batch_dispatch_statuses(message_ids, events).await?;

    // Check no runtime panic occurred
    assert!(api.is_progressing().await?);

    Ok(())
}

#[tokio::test]
async fn harmless_upload() -> Result<()> {
    let examples = vec![
        WatExample::WrongLoad,
        WatExample::InfRecursion,
        WatExample::ReadAccess,
        WatExample::ReadWriteAccess,
        WatExample::from_wat(use_big_memory_wat())
            .expect("Cannot create wat example for big memory test"),
    ];

    let codes = examples.into_iter().map(|e| e.code()).collect();

    // Creating gear api.
    let (_node, api) = dev_node().await;
    let api = api.unsigned().clone().signed_dev(AccountKeyring::Bob);

    upload_programs_and_check(&api, codes, None).await?;

    Ok(())
}

#[tokio::test]
async fn alloc_zero_pages() -> Result<()> {
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

    let (_node, api) = dev_node().await;
    let api = api.unsigned().clone().signed_dev(AccountKeyring::Bob);

    let codes = vec![wat::parse_str(wat_code).unwrap()];
    upload_programs_and_check(&api, codes, Some(Duration::from_secs(15))).await
}

#[tokio::test]
async fn get_mailbox() -> Result<()> {
    // Creating gear api.
    //
    // By default, login as Alice, than re-login as Bob.
    let (_node, api) = dev_node().await;
    let api = api.unsigned().clone().signed_dev(AccountKeyring::Bob);

    // Subscribe to events
    let mut events = pin!(api.subscribe_all_events().await?);

    // Check that blocks are still running
    assert!(api.is_progressing().await?);

    let wat_code = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_source" (func $source (param i32)))
        (import "env" "gr_send_init" (func $send_init (param i32)))
        (import "env" "gr_send_push" (func $send_push (param i32 i32 i32 i32)))
        (import "env" "gr_send_commit_wgas" (func $send_commit (param i32 i32 i64 i32 i32)))
        (export "handle" (func $handle))
        (func $handle
            ;; getting source of the program
            (call $source (i32.const 0xfa00))

            ;; getting new sending handle
            ;; handle will has addr 0xfa34
            (call $send_init (i32.const 0xfa30))

            ;; pushing payload
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))
            (call $send_push (i32.load (i32.const 0xfa34)) (i32.const 0) (i32.const 0xfa00) (i32.const 0xfa38))

            ;; sending commit
            (call $send_commit (i32.load (i32.const 0xfa34)) (i32.const 0xfa00) (i64.const 100000) (i32.const 0) (i32.const 0xfa38))
        )
        (data (i32.const 0) "PONG")
    )"#;

    let code = wat::parse_str(wat_code).unwrap();

    // Calculate gas amount needed for initialization
    let gas_info = api
        .calculate_upload_gas(code.clone(), vec![], 0, true)
        .await?;

    // Upload and init the program
    let (message_id, program_id) = api
        .upload_program_bytes(
            code,
            gear_utils::now_micros().to_le_bytes(),
            vec![],
            gas_info.min_limit,
            0,
        )
        .await?
        .value;

    assert!(
        events::message_dispatch_status(message_id, &mut events)
            .await?
            .is_success()
    );

    // Calculate gas amount needed for handling the message
    let gas_info = api
        .calculate_handle_gas(program_id, vec![], 0, true)
        .await?;

    let messages = vec![(program_id, vec![], gas_info.min_limit * 10, 0); 5];

    let messages = api.send_message_bytes_batch(messages).await?.value;

    let (message_id, _) = *messages.last().unwrap().as_ref().unwrap();

    assert!(
        events::message_dispatch_status(message_id, &mut events)
            .await?
            .is_success()
    );

    let mailbox = api.mailbox_messages(15).await?;

    // Check that all messages is in mailbox
    assert_eq!(mailbox.len(), 5);

    for msg in mailbox {
        assert_eq!(msg.0.payload_bytes().len(), 1000 * 1024); // 1MB payload
        assert!(msg.0.payload_bytes().starts_with(b"PONG"));
    }

    Ok(())
}

#[tokio::test]
async fn test_init_message() -> Result<()> {
    let (_node, api) = dev_node().await;

    let events = api.subscribe_all_events().await?;

    let (message_id, _) = api
        .upload_program_bytes(
            demo_messenger::WASM_BINARY,
            gear_utils::now_micros().to_le_bytes(),
            vec![],
            api.block_gas_limit()?,
            0,
        )
        .await?
        .value;

    events::message_dispatch_status(message_id, events).await?;

    let messages = api.mailbox_messages(10).await?;

    assert_eq!(messages.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_upload_failed() -> Result<()> {
    let (_node, api) = dev_node().await;

    let err = api
        .upload_program_bytes(vec![], vec![], b"", u64::MAX, 0)
        .await
        .expect_err("Should fail");

    assert!(
        matches!(
            err,
            gsdk::Error::Runtime(RuntimeError::Gear(gear::Error::GasLimitTooHigh))
        ),
        "{err:?}"
    );

    Ok(())
}

#[tokio::test]
async fn upload_large_scheduled() -> Result<()> {
    let (_node, api) = dev_node().await;

    // Taking block gas limit constant.
    let gas_limit = api.block_gas_limit()?;

    // Subscribing for events.
    let events = api.subscribe_all_events().await?;

    let code = WatExample::LargeScheduled.code();

    // Program initialization.
    let (message_id, _pid) = api
        .upload_program_bytes(
            code,
            gear_utils::now_micros().to_le_bytes(),
            "",
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

fn use_big_memory_wat() -> String {
    let last_4_bytes_offset = WasmPage::from(MAX_WASM_PAGES_AMOUNT).offset() - 4;
    let middle_4_bytes_offset = WasmPage::from(MAX_WASM_PAGES_AMOUNT / 2).offset();

    format!(
        r#"
        (module
		    (import "env" "memory" (memory 0))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (export "init" (func $init))
            (func $init
                (drop (call $alloc (i32.const {MAX_WASM_PAGES_AMOUNT})))

                ;; access last 4 bytes
                (i32.store (i32.const {last_4_bytes_offset}) (i32.const 0x42))

                ;; access first 4 bytes
                (i32.store (i32.const 0) (i32.const 0x42))

                ;; access 4 bytes in the middle
                (i32.store (i32.const {middle_4_bytes_offset}) (i32.const 0x42))
            )
        )"#
    )
}
