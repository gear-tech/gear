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

use gclient::{code_from_os, now_micros, Error, EventListener, EventProcessor, GearApi, Result};
use gear_core::ids::ProgramId;
use gsdk::ext::sp_runtime::MultiAddress;
use hex::ToHex;
use parity_scale_codec::{Decode, Encode};

async fn charge_10(
    api: &GearApi,
    program_id: ProgramId,
    listener: &mut EventListener,
) -> Result<String> {
    let payload = b"10".to_vec();
    let gas_info = api
        .calculate_handle_gas(None, program_id, payload.clone(), 0, true)
        .await?;
    let (message_id, _hash) = api
        .send_message_bytes(program_id, payload, gas_info.min_limit, 0)
        .await?;
    assert!(listener.message_processed(message_id).await?.succeed());

    let msg = api.get_mailbox_messages(1).await.unwrap().pop();
    if let Some(msg) = msg {
        let message = api
            .get_mailbox_message(msg.0.id())
            .await
            .unwrap()
            .unwrap()
            .0;

        api.claim_value(msg.0.id()).await.unwrap();

        return Ok(String::from_utf8(message.payload().to_vec()).unwrap());
    }

    Ok(String::new())
}

struct CleanupFolderOnDrop {
    path: String,
}

impl Drop for CleanupFolderOnDrop {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).expect("Failed to cleanup after test")
    }
}

#[tokio::test]
async fn memory_dump() -> Result<()> {
    // Create API instance
    let api = GearApi::dev().await?;
    // Subscribe to events
    let mut listener = api.subscribe().await?;
    // Check that blocks are still running
    assert!(listener.blocks_running().await?);
    // Calculate gas amount needed for initialization
    let gas_info = api
        .calculate_upload_gas(
            None,
            code_from_os("../target/wasm32-unknown-unknown/release/demo_capacitor.wasm")?,
            vec![],
            0,
            true,
        )
        .await?;
    // Upload and init the program
    let (message_id, program_id, _hash) = api
        .upload_program_bytes_by_path(
            "../target/wasm32-unknown-unknown/release/demo_capacitor.wasm",
            now_micros().to_le_bytes(),
            b"15".to_vec(),
            gas_info.min_limit,
            0,
        )
        .await?;
    assert!(listener.message_processed(message_id).await?.succeed());

    assert_eq!(
        charge_10(&api, program_id, &mut listener).await.unwrap(),
        ""
    );

    let cleanup = CleanupFolderOnDrop {
        path: "./296c6962726".to_string(),
    };

    api.save_program_memory_dump_at(program_id, None, "./296c6962726/demo_capacitor.dump")
        .await
        .unwrap();

    assert_eq!(
        charge_10(&api, program_id, &mut listener).await.unwrap(),
        "Discharged: 20"
    );

    api.replace_program_memory(program_id, "./296c6962726/demo_capacitor.dump")
        .await
        .unwrap();

    drop(cleanup);

    assert_eq!(
        charge_10(&api, program_id, &mut listener).await.unwrap(),
        "Discharged: 20"
    );

    Ok(())
}
