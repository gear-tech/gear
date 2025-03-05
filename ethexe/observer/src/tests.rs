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
use alloy::{
    network::TransactionBuilder,
    node_bindings::Anvil,
    primitives::U256,
    providers::WsConnect,
    rpc::types::TransactionRequest,
    transports::{RpcError, TransportErrorKind},
};
use ethexe_db::{Database, MemDb};
use ethexe_ethereum::Ethereum;
use ethexe_signer::Signer;
use gprimitives::ActorId;
use roast_secp256k1_evm::frost::{
    keys::{self, IdentifierList},
    Identifier,
};
use std::{collections::HashMap, time::Duration};

fn wat2wasm_with_validate(s: &str, validate: bool) -> Vec<u8> {
    let code = wat::parse_str(s).unwrap();
    if validate {
        wasmparser::validate(&code).unwrap();
    }
    code
}

fn wat2wasm(s: &str) -> Vec<u8> {
    wat2wasm_with_validate(s, true)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_deployment() -> Result<()> {
    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;

    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::tmp();

    let sender_public_key = signer
        .add_key("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let (secret_shares, _) = keys::generate_with_dealer(
        1,
        1,
        IdentifierList::Custom(&[Identifier::deserialize(
            &ActorId::from(validators[0]).into_bytes(),
        )
        .unwrap()]),
        rand::thread_rng(),
    )
    .unwrap();

    let verifiable_secret_sharing_commitment = secret_shares
        .values()
        .map(|secret_share| secret_share.commitment().clone())
        .next()
        .expect("conversion failed");

    let ethereum = Ethereum::deploy(
        &ethereum_rpc,
        validators,
        signer,
        sender_address,
        verifiable_secret_sharing_commitment,
    )
    .await?;

    let blobs_reader = Arc::new(MockBlobReader::new());

    let router_address = ethereum.router().address();

    let db = MemDb::default();
    let database = Database::from_one(&db, router_address.0);

    let mut observer = ObserverService::new(
        &EthereumConfig {
            rpc: ethereum_rpc,
            router_address,
            block_time: Duration::from_secs(1),
            beacon_rpc: Default::default(),
        },
        u32::MAX,
        database,
        Some(blobs_reader.clone()),
    )
    .await
    .expect("failed to create observer");

    let request_wasm_validation = async move |wasm: Vec<u8>| {
        let pending_builder = ethereum
            .router()
            .request_code_validation_with_sidecar(&wasm)
            .await
            .expect("failed to request code validation");

        let request_code_id = pending_builder.code_id();
        let request_tx_hash = pending_builder.tx_hash();

        blobs_reader
            .add_blob_transaction(request_tx_hash, wasm)
            .await;

        request_code_id
    };

    let mut observer_next = async move || {
        observer
            .next()
            .await
            .expect("observer did not receive event")
            .expect("received error instead of event")
    };

    let mut expected_events = HashMap::new();
    let blobs_amount = 20;
    for i in 0..blobs_amount {
        let wat = format!(
            r#"(module
                (import "env" "memory" (memory 1))
                (export "init" (func $init))
                (export "ret_{0}" (func $ret_{0}))
                (func $init (nop))
                (func $ret_{0} (result i32)
                    i32.const {0}
                ))"#,
            i
        );
        let wasm = wat2wasm(&wat);
        let request_code_id = request_wasm_validation(wasm.clone()).await;
        expected_events.insert(request_code_id, wasm);
    }

    let result = tokio::time::timeout(tokio::time::Duration::from_secs(2), async move {
        while !expected_events.is_empty() {
            let event = observer_next().await;
            if let ObserverEvent::Blob {
                code_id,
                timestamp: _,
                code,
            } = event
            {
                let expected_code = expected_events
                    .remove(&code_id)
                    .expect("Expect event exists");
                assert_eq!(code, expected_code);
            }
        }
    })
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(_) => Err(anyhow!("Expected all events will be process for 2 seconds")),
    }
}

#[tokio::test]
async fn test_node_disconnect() -> Result<()> {
    let port = 8454u16;
    let anvil = Anvil::new().port(port).try_spawn()?;
    let provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(anvil.ws_endpoint_url()))
        .await?;

    let block = provider.get_block_number().await?;

    // rerun node to test the reconnection
    drop(anvil);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let anvil = Anvil::new().port(port).try_spawn()?;

    // assert that provider become invalide after node rerun
    assert!(
        matches!(
            provider.get_block_number().await,
            Err(RpcError::Transport(TransportErrorKind::BackendGone))
        ),
        "Expect `BackendGone` error after anvil rerun"
    );

    // create new provider
    let provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(anvil.ws_endpoint_url()))
        .await?;

    let block_after_rerun = provider.get_block_number().await?;
    assert_eq!(block, block_after_rerun);

    Ok(())
}

#[tokio::test]
async fn test_blocks_resubscribing() -> Result<()> {
    gear_utils::init_default_logger();

    let port = 8455u16;
    let anvil = Anvil::new().port(port).try_spawn()?;
    let provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(anvil.ws_endpoint_url()))
        .await?;

    let accounts = provider.get_accounts().await?;
    assert!(accounts.len() >= 2);

    let (alice, bob) = (accounts[0], accounts[1]);

    let subscription = provider.subscribe_blocks().await?;
    let mut stream = subscription.resubscribe().into_stream();

    let tx = TransactionRequest::default()
        .with_from(alice)
        .with_to(bob)
        .with_value(U256::from(1));
    let _tx_hash = provider.send_transaction(tx).await?.watch().await?;

    assert!(
        stream.next().await.is_some(),
        "Expect a block header after tx"
    );

    drop(anvil);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let anvil = Anvil::new().port(port).try_spawn()?;

    let provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(anvil.ws_endpoint_url()))
        .await?;

    assert_eq!(
        stream.next().await,
        None,
        "Expect None, because stream is terminate"
    );

    // recreate subscription because previous one will return the terminated stream
    let subscription = provider.subscribe_blocks().await?;
    let mut stream = subscription.resubscribe().into_stream();

    let tx = TransactionRequest::default()
        .with_from(bob)
        .with_to(alice)
        .with_value(U256::from(1));

    let _tx_hash = provider
        .send_transaction(tx)
        .await
        .expect("Successfull send tx")
        .watch()
        .await
        .expect("Successfull watch tx");

    assert!(
        stream.next().await.is_some(),
        "Expect a block header after tx"
    );

    Ok(())
}
