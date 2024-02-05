// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use demo_meta_io::Wallet;
use gclient::{EventProcessor, GearApi};
use gmeta::MetadataRepr;
use parity_scale_codec::{Decode, Encode};

#[tokio::test]
async fn get_state() -> anyhow::Result<()> {
    let api = GearApi::dev_from_path("../target/release/gear").await?;

    // Subscribe to events
    let mut listener = api.subscribe().await?;

    // Check that blocks are still running
    assert!(listener.blocks_running().await?);

    // Calculate gas amount needed for initialization
    let gas_info = api
        .calculate_upload_gas(None, demo_new_meta::WASM_BINARY.to_vec(), vec![], 0, true)
        .await?;

    // Upload and init the program
    let (message_id, program_id, _hash) = api
        .upload_program_bytes(
            demo_new_meta::WASM_BINARY,
            gclient::now_micros().to_le_bytes(),
            vec![],
            gas_info.min_limit,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Read and check `metahash`
    let actual_metahash = api.read_metahash(program_id).await?.0;

    let expected_metahash = MetadataRepr::from_bytes(demo_new_meta::WASM_METADATA)
        .expect("Failed to read meta from bytes")
        .hash();

    assert_eq!(actual_metahash, expected_metahash);

    // Read state bytes
    let state = api.read_state_bytes(program_id, Default::default()).await?;
    let wallets = Vec::<Wallet>::decode(&mut state.as_ref()).expect("Unable to decode");
    assert_eq!(wallets.len(), 2);

    // Read state using Wasm
    let wallet: Option<Wallet> = api
        .read_state_using_wasm(
            program_id,
            Default::default(),
            "first_wallet",
            demo_new_meta::META_WASM_V1.to_vec(),
            <Option<()>>::None,
        )
        .await?;
    let wallet = wallet.expect("No wallet");

    assert_eq!(wallet.id.decimal, 1);
    assert_eq!(wallet.person.surname, "SomeSurname");
    assert_eq!(wallet.person.name, "SomeName");

    Ok(())
}

#[tokio::test]
async fn get_state_request() -> anyhow::Result<()> {
    use demo_custom::{btree, InitMessage, WASM_BINARY};

    let gas_limit = 100_000_000_000;

    let api = GearApi::dev_from_path("../target/release/gear").await?;

    // Or use this comment to run test on custom node
    // let api = GearApi::dev().await?.with("//Alice")?;

    // Subscribe to events
    let mut listener = api.subscribe().await?;

    // Check that blocks are still running
    assert!(listener.blocks_running().await?);

    // Upload btree program and wait initialization is done
    let (message_id, program_id, _hash) = api
        .upload_program_bytes(
            WASM_BINARY,
            gclient::now_micros().to_le_bytes(),
            InitMessage::BTree.encode(),
            gas_limit,
            0,
        )
        .await?;
    assert!(listener.message_processed(message_id).await?.succeed());

    let data = [(0u32, 1u32), (1, 2), (3, 4)];
    let batch = data.map(|(key, value)| {
        (
            program_id,
            btree::Request::Insert(key, value).encode(),
            gas_limit,
            0,
        )
    });

    // Store some data in btree and wait the results
    let message_ids = api
        .send_message_bytes_batch(batch)
        .await
        .unwrap()
        .0
        .into_iter()
        .map(|res| res.unwrap().0);
    listener
        .message_processed_batch(message_ids.into_iter())
        .await?
        .into_iter()
        .for_each(|(_, status)| assert!(status.succeed()));

    // Check state can be read by one key
    for (key, value) in data {
        let res: Option<u32> = api
            .read_state(program_id, btree::StateRequest::ForKey(key).encode())
            .await?;
        assert_eq!(res, Some(value));
    }

    Ok(())
}
