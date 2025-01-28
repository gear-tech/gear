// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use super::*;
use crate::MockBlobReader;
use alloy::node_bindings::Anvil;
use ethexe_ethereum::Ethereum;
use ethexe_signer::Signer;
use std::time::Duration;

fn wat2wasm_with_validate(s: &str, validate: bool) -> Vec<u8> {
    wabt::Wat2Wasm::new()
        .validate(validate)
        .convert(s)
        .unwrap()
        .as_ref()
        .to_vec()
}

fn wat2wasm(s: &str) -> Vec<u8> {
    wat2wasm_with_validate(s, true)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_deployment() -> Result<()> {
    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;

    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::new("/tmp/keys".into())?;

    let sender_public_key = signer
        .add_key("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let ethereum = Ethereum::deploy(&ethereum_rpc, validators, signer, sender_address).await?;
    let blob_reader = Arc::new(MockBlobReader::new(&ethereum_rpc, Duration::from_secs(1)).await?);

    let router_address = ethereum.router().address();
    let cloned_blob_reader = blob_reader.clone();

    let config = EthereumConfig {
        rpc: ethereum_rpc.clone(),
        router_address,
        block_time: Duration::from_secs(1),
        beacon_rpc: Default::default(),
    };

    let mut observer = ObserverService::new_with_blobs(&config, cloned_blob_reader)
        .await
        .expect("failed to create observer");

    let wat = r#"
            (module
                (import "env" "memory" (memory 0))
                (export "init" (func $init))
                (func $init)
            )
        "#;
    let wasm = wat2wasm(wat);

    let pending_builder = ethereum
        .router()
        .request_code_validation_with_sidecar(&wasm)
        .await?;

    let request_code_id = pending_builder.code_id();
    let request_tx_hash = pending_builder.tx_hash();

    blob_reader
        .add_blob_transaction(request_tx_hash, wasm.clone())
        .await;

    let event = observer
        .next()
        .await
        .expect("observer did not receive event");

    assert!(matches!(event, ObserverEvent::Block(..)));

    let event = observer
        .next()
        .await
        .expect("observer did not receive event");

    assert!(matches!(
        event,
        ObserverEvent::Blob {
            code_id,
            code,
            ..
        }
        if code_id == request_code_id && code == wasm
    ));

    Ok(())
}
