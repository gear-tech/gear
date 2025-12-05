// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use std::{collections::BTreeSet, ops::Deref, path::PathBuf};

use demo_custom::{InitMessage, WASM_BINARY};
use gear_core::{ids::ActorId, pages::GearPage};
use gear_node_wrapper::Node;
use gsdk::{Api, Result, SignedApi, blocks, events};
use parity_scale_codec::Encode;
use tokio::fs;
use utils::dev_node;

mod utils;

async fn charge_10(api: &SignedApi, program_id: ActorId) -> Result<String> {
    let payload = b"10".to_vec();
    let gas_info = api
        .calculate_handle_gas(program_id, payload.clone(), 0, true)
        .await?;

    let events = api.subscribe_all_events().await.unwrap();

    let message_id = api
        .send_message_bytes(program_id, payload, gas_info.min_limit, 0)
        .await?
        .value;

    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_success()
    );

    let msg = api.mailbox_messages(1).await.unwrap().pop();
    if let Some(msg) = msg {
        let message = api.mailbox_message(msg.0.id()).await.unwrap().unwrap().0;

        api.claim_value(msg.0.id()).await.unwrap();

        return Ok(String::from_utf8(message.payload_bytes().to_vec()).unwrap());
    }

    Ok(String::new())
}

struct CleanupFolderOnDrop {
    path: PathBuf,
}

impl Drop for CleanupFolderOnDrop {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).expect("Failed to cleanup after test")
    }
}

#[tokio::test]
async fn memory_dump() -> Result<()> {
    let node = Node::from_path("../target/release/gear")
        .unwrap()
        .spawn()
        .unwrap();

    // Create API instance
    let api = Api::new(&node.ws()).await?.signed_as_alice();
    // Subscribe to events
    let events = api.subscribe_all_events().await?;
    // Check that blocks are still running
    assert!(blocks::are_progressing(api.blocks().subscribe_all().await?).await?);
    // Calculate gas amount needed for initialization
    let payload = InitMessage::Capacitor("15".to_string());
    let gas_info = api
        .calculate_upload_gas(WASM_BINARY.to_vec(), payload.encode(), 0, true)
        .await?;
    // Upload and init the program
    let (message_id, program_id) = api
        .upload_program(
            WASM_BINARY,
            gear_utils::now_micros().to_le_bytes(),
            payload,
            gas_info.min_limit,
            0,
        )
        .await?
        .value;
    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_success()
    );

    assert_eq!(charge_10(&api, program_id).await.unwrap(), "");

    let dir = PathBuf::from("./296c6962726");

    fs::create_dir_all(&dir).await.unwrap();

    let cleanup = CleanupFolderOnDrop { path: dir.clone() };

    api.save_program_memory_dump_at(program_id, None, dir.join("demo_custom.dump"))
        .await
        .unwrap();

    assert_eq!(charge_10(&api, program_id).await.unwrap(), "Discharged: 20");

    api.replace_program_memory(program_id, dir.join("demo_custom.dump"))
        .await
        .unwrap();

    drop(cleanup);

    assert_eq!(charge_10(&api, program_id).await.unwrap(), "Discharged: 20");

    Ok(())
}

#[tokio::test]
async fn memory_download() -> Result<()> {
    // Create API instance
    let (_node, api) = dev_node().await;

    // Check that blocks are still running
    assert!(blocks::are_progressing(api.blocks().subscribe_all().await?).await?);

    let wat = r#"
        (module
            (import "env" "memory" (memory 512))
            (export "init" (func $init))
            (func $init
                (local $counter i32)

                (loop
                    (i32.store
                        (i32.mul (local.get $counter) (i32.const 0x8000))
                        (i32.const 0x42)
                    )

                    (i32.add (local.get $counter) (i32.const 1))
                    local.tee $counter

                    i32.const 1000
                    i32.lt_u
                    br_if 0
                )
            )
        )
    "#;

    let wasm = wat::parse_str(wat).unwrap();

    let events = api.subscribe_all_events().await?;

    // Upload and init the program
    let (message_id, program_id) = api
        .upload_program_bytes(
            wasm,
            gear_utils::now_micros().to_le_bytes(),
            Vec::new(),
            200_000_000_000,
            0,
        )
        .await?
        .value;

    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_success()
    );

    let timer_start = gear_utils::now_micros();
    let pages = api.program_pages(program_id).await?;
    let timer_end = gear_utils::now_micros();

    println!(
        "Storage prefix iteration memory download took: {} ms",
        (timer_end - timer_start) / 1000
    );

    let mut accessed_pages = BTreeSet::new();
    let mut expected_data = [0u8; 0x4000];
    expected_data[0] = 0x42;
    for (page, data) in pages {
        accessed_pages.insert(page);
        assert_eq!(data.deref(), expected_data.as_slice());
    }

    assert_eq!(
        accessed_pages,
        (0..1000)
            .map(|p| p * 2)
            .map(Into::into)
            .collect::<BTreeSet<GearPage>>()
    );

    let timer_start = gear_utils::now_micros();
    let _pages = api
        .specified_program_pages(program_id, accessed_pages)
        .await?;
    let timer_end = gear_utils::now_micros();

    println!(
        "Memory page by page download took: {} ms",
        (timer_end - timer_start) / 1000
    );

    Ok(())
}
