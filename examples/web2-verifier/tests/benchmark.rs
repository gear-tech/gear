// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(test)]

pub mod utils;
use utils::*;

use demo_web2_verifier::{Report, WASM_BINARY};
use gclient::{EventProcessor, GearApi, Result};
use gstd::{Encode, Vec, vec};

/// Path to the gear node binary.
const GEAR_PATH: &str = "../../target/release/gear";

const MAX_GAS_LIMIT: u64 = 250_000_000_000;

#[tokio::test]
async fn make_request_and_verify_result_test() -> Result<()> {
    let request_opts = [
        ("gear.com", collector::Opts {
            url: "https://gear.com".into(),
            ..Default::default()
        }),
        ("gear.com", collector::Opts {
            url: "https://gear.com".into(),
            ..Default::default()
        }),
        ("sandbox-api.coinmarketcap.com", collector::Opts {
            url: "https://sandbox-api.coinmarketcap.com/v1/cryptocurrency/listings/latest?start=1&limit=5000&convert=USD".into(),
            headers: vec!["X-CMC_PRO_API_KEY: b54bcf4d-1bca-4e8e-9a24-22ff2c3d462c".into(), "Accept: application/json".into()],
            method: "GET".into(),
            ..Default::default()
        })
    ];

    let api = GearApi::dev_from_path(GEAR_PATH).await?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Uploading program.
    let (message_id, program_id, _hash) = api
        .upload_program_bytes(WASM_BINARY, [137u8], vec![], MAX_GAS_LIMIT, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    for (sni, opts) in request_opts {
        let artifact = collector::collect_request(opts).await?;
        let payload = artifact.encode();

        // Calculating gas info.
        let gas_info = api
            .calculate_handle_gas(None, program_id, payload.clone(), 0, true)
            .await?;

        println!("Gas: {gas_info:?}");

        // Sending message with prepeared payload
        let (message_id, _hash) = api
            .send_message_bytes(program_id, payload, MAX_GAS_LIMIT, 0)
            .await?;

        assert!(listener.message_processed(message_id).await?.succeed());

        let sni = sni.as_bytes().to_vec().encode();

        // Reading state with prepeared sni for payload
        let res: Vec<Report> = api.read_state(program_id, sni).await?;

        println!("Result: {res:?}");
    }

    Ok(())
}
