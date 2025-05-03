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
use alloy::node_bindings::Anvil;
use ethexe_blob_loader::{blobs::MockBlobReader, BlobLoaderEvent, BlobLoaderService};
use ethexe_db::{Database, MemDb};
use ethexe_ethereum::Ethereum;
use ethexe_signer::Signer;
use gprimitives::ActorId;
use roast_secp256k1_evm::frost::{
    keys::{self, IdentifierList},
    Identifier,
};
use std::time::Duration;

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

    let blobs_reader = MockBlobReader::new();

    let router_address = ethereum.router().address();

    let db = MemDb::default();
    let database = Database::from_one(&db);

    let mut observer = ObserverService::new(
        &EthereumConfig {
            rpc: ethereum_rpc,
            router_address,
            block_time: Duration::from_secs(1),
            beacon_rpc: Default::default(),
        },
        u32::MAX,
        database.clone(),
        blobs_reader.clone_boxed(),
    )
    .await
    .expect("failed to create observer");

    let mut blob_loader = BlobLoaderService::new(blobs_reader.clone_boxed(), database);

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

    // let mut observer_next = async move || {
    //     observer
    //         .next()
    //         .await
    //         .expect("observer did not receive event")
    //         .expect("received error instead of event")
    // };

    let wat = r#"
        (module
            (import "env" "memory" (memory 0))
            (export "init" (func $init))
            (func $init)
        )
    "#;
    let wasm = wat2wasm(wat);
    let request_code_id = request_wasm_validation(wasm.clone()).await;

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");

    log::info!("Event received: {event:?}");
    assert!(matches!(event, ObserverEvent::Block(..)));

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    let ObserverEvent::RequestLoadBlobs(codes) = event else {
        panic!("Expected event: ObserverEvent::RequestLoadBlobs, received: {event:?}");
    };

    blob_loader.load_codes(codes, None)?;
    let event = blob_loader
        .next()
        .await
        .expect("BlobLoader didn't receive event")
        .expect("received error insted ob event");
    log::info!("Event received: {event:?}");
    let BlobLoaderEvent::BlobLoaded(blob_data) = event else {
        panic!("Expected event: BlobLoaderEvent::BlobLoaded, received: {event:?}");
    };

    observer.receive_loaded_blob(blob_data);
    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    assert!(
        matches!(event, ObserverEvent::Blob(d) if d.code_id == request_code_id && d.code == wasm)
    );

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    assert!(matches!(event, ObserverEvent::BlockSynced(..)));

    // let event = observer
    //     .next()
    //     .await
    //     .expect("observer did not receive event")
    //     .expect("received error instead of event");
    // log::info!("Event received: {event:?}");
    // assert!(
    //     matches!(event, ObserverEvent::Blob(d) if d.code_id == request_code_id && d.code == wasm)
    // );

    let wat = "(module)";
    let wasm = wat2wasm(wat);
    let request_code_id = request_wasm_validation(wasm.clone()).await;

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    assert!(matches!(event, ObserverEvent::Block(..)));

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    let ObserverEvent::RequestLoadBlobs(codes) = event else {
        panic!("Expected event: ObserverEvent::RequestLoadBlobs, received: {event:?}");
    };

    blob_loader.load_codes(codes, None)?;
    let event = blob_loader
        .next()
        .await
        .expect("BlobLoader didn't receive event")
        .expect("received error insted ob event");
    log::info!("Event received: {event:?}");
    let BlobLoaderEvent::BlobLoaded(blob_data) = event else {
        panic!("Expected event: BlobLoaderEvent::BlobLoaded, received: {event:?}");
    };

    observer.receive_loaded_blob(blob_data);

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    log::info!("Event received: {event:?}");
    assert!(
        matches!(event, ObserverEvent::Blob(d) if d.code_id == request_code_id && d.code == wasm)
    );

    Ok(())
}
