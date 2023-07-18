// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use parity_scale_codec::Decode;

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
    let state = api.read_state_bytes(program_id).await?;
    let wallets = Vec::<Wallet>::decode(&mut state.as_ref()).expect("Unable to decode");
    assert_eq!(wallets.len(), 2);

    // Read state using Wasm
    let wallet: Option<Wallet> = api
        .read_state_using_wasm(
            program_id,
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
