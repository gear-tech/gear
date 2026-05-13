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
use alloy::{node_bindings::Anvil, providers::ext::AnvilApi, pubsub::RawSubscription};
use ethexe_db::InitConfig;
use ethexe_ethereum::deploy::EthereumDeployer;
use futures::future::poll_fn;
use gsigner::secp256k1::Signer;
use std::task::Poll;
use tokio::time::{Duration, timeout};

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

async fn create_observer(ethereum_rpc: &str, router_address: Address) -> Result<ObserverService> {
    let database = ethexe_db::create_initialized_empty_memory_db(InitConfig {
        ethereum_rpc: ethereum_rpc.to_owned(),
        router_address,
        slot_duration_secs: 1,
        genesis_initializer: None,
    })
    .await?;

    ObserverService::new(
        database,
        ObserverConfig {
            rpc: ethereum_rpc,
            max_sync_depth: None,
        },
    )
    .await
}

#[tokio::test]
async fn test_deployment() -> Result<()> {
    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;
    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::memory();
    let sender_public_key = signer
        .import("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators: Vec<Address> = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let deployer = EthereumDeployer::new(&ethereum_rpc, signer, sender_address)
        .await
        .unwrap();
    let ethereum = deployer
        .with_validators(validators.try_into().unwrap())
        .deploy()
        .await?;

    let mut observer = create_observer(&ethereum_rpc, ethereum.router().address())
        .await
        .expect("failed to create observer");

    let request_wasm_validation = async move |wasm: Vec<u8>| {
        let (_tx_hash, code_id) = ethereum
            .router()
            .request_code_validation(&wasm)
            .await
            .expect("failed to request code validation");

        code_id
    };

    let wat = r#"
        (module
            (import "env" "memory" (memory 0))
            (export "init" (func $init))
            (func $init)
        )
    "#;
    let wasm = wat2wasm(wat);
    let _request_code_id = request_wasm_validation(wasm).await;

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");

    assert!(matches!(event, ObserverEvent::Block(..)));

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");

    let ObserverEvent::BlockSynced { .. } = event else {
        panic!("Expected event: ObserverEvent::RequestLoadBlobs, received: {event:?}");
    };

    let wat = "(module)";
    let wasm = wat2wasm(wat);
    let _request_code_id = request_wasm_validation(wasm).await;

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    assert!(matches!(event, ObserverEvent::Block(..)));

    let event = observer
        .next()
        .await
        .expect("observer did not receive event")
        .expect("received error instead of event");
    let ObserverEvent::BlockSynced { .. } = event else {
        panic!("Expected event: ObserverEvent::RequestLoadBlobs, received: {event:?}");
    };

    Ok(())
}

#[tokio::test]
async fn resubscribes_when_headers_stream_terminates() -> Result<()> {
    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;
    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::memory();
    let sender_public_key = signer
        .import("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators: Vec<Address> = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let deployer = EthereumDeployer::new(&ethereum_rpc, signer, sender_address)
        .await
        .unwrap();
    let ethereum = deployer
        .with_validators(validators.try_into().unwrap())
        .deploy()
        .await?;

    let mut observer = create_observer(&ethereum_rpc, ethereum.router().address())
        .await
        .expect("failed to create observer");

    let (tx, rx) = tokio::sync::broadcast::channel(1);
    drop(tx);
    observer.headers_stream = RawSubscription {
        rx,
        local_id: Default::default(),
    }
    .into_typed::<Header>()
    .into_stream();

    let provider = observer.provider().clone();

    let mut resubscribe_started = false;
    timeout(
        Duration::from_secs(10),
        poll_fn(|cx| {
            let _ = Pin::new(&mut observer).poll_next(cx);

            if observer.subscription_future.is_some() {
                resubscribe_started = true;
            }

            if resubscribe_started && observer.subscription_future.is_none() {
                Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }),
    )
    .await
    .expect("observer did not recreate headers subscription in time");

    provider.anvil_mine(Some(1), None).await?;

    let event = timeout(Duration::from_secs(10), observer.next())
        .await
        .expect("observer did not receive a block from recreated subscription in time")
        .expect("observer stream ended")
        .expect("received error instead of event");

    assert!(matches!(event, ObserverEvent::Block(..)));

    Ok(())
}

/// Regression guard for the `ensure_validators` reorg gap pinned by
/// Opus review #3.
///
/// `ChainSync::ensure_validators` makes pinned `eth_call` queries via
/// `router_query.validators_at(block_hash)` and
/// `middleware_query.make_election_at(...)`. When the chain reorgs
/// out the block hash these calls are pinned at, the node responds
/// with a reorg-flavoured RPC error wrapped inside
/// `alloy::contract::Error -> alloy::transports::RpcError`. The
/// original classifier was only wired into `EthereumBlockLoader::load`
/// (the `eth_getLogs` path), so the `ensure_validators` failure
/// crashed the service.
///
/// The fix moved classification to a single point — `SyncError`'s
/// `From<anyhow::Error>` walks the source chain looking for any
/// `alloy::transports::RpcError`. This test exercises the
/// `validators_at` path directly and checks the classifier still
/// catches it.
#[tokio::test]
async fn validators_at_on_orphaned_block_is_recoverable_rpc_error() -> Result<()> {
    use crate::SyncError;

    gear_utils::init_default_logger();

    let anvil = Anvil::new().try_spawn()?;
    let ethereum_rpc = anvil.ws_endpoint();

    let signer = Signer::memory();
    let sender_public_key = signer
        .import("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?)?;
    let sender_address = sender_public_key.to_address();
    let validators: Vec<Address> = vec!["0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1".parse()?];

    let deployer = EthereumDeployer::new(&ethereum_rpc, signer, sender_address)
        .await
        .unwrap();
    let ethereum = deployer
        .with_validators(validators.try_into().unwrap())
        .deploy()
        .await?;

    let router_query = ethereum.router().query();

    // Take a snapshot at the post-deploy tip. Anything we mine after
    // this point will be reverted out.
    let provider = ethereum.provider();
    let snapshot_id = provider.anvil_snapshot().await?;

    // Mine a fresh block and record its hash. `validators_at` calls
    // succeed against this hash *before* the revert (sanity check).
    provider.anvil_mine(Some(1), None).await?;
    let orphaned_block = provider
        .get_block(alloy::eips::BlockId::latest())
        .await?
        .expect("latest block exists after anvil_mine");
    let orphaned_hash: H256 = orphaned_block.header.hash.0.into();

    router_query
        .validators_at(orphaned_hash)
        .await
        .expect("validators_at must succeed before the revert");

    // Revert anvil to the snapshot — `orphaned_hash` is no longer
    // canonical and the node returns the reorg-flavoured error
    // family we're after.
    let reverted = provider.anvil_revert(snapshot_id).await?;
    assert!(reverted, "anvil_revert must accept the snapshot id");

    let err = router_query
        .validators_at(orphaned_hash)
        .await
        .expect_err("validators_at must error on a block that was reorged out");

    // The fix: `SyncError`'s anyhow conversion walks the source chain
    // for an `alloy::transports::RpcError` and classifies it as
    // recoverable. `ObserverService::poll_next` then logs a warning,
    // bumps the counter, and waits for the next chain head instead
    // of crashing.
    let classified = SyncError::from(err);
    match classified {
        SyncError::RpcError(_) => { /* expected */ }
        SyncError::Fatal(err) => panic!(
            "regression: validators_at error on an orphaned block is NOT classified as \
             recoverable — service will crash on every reorg through ensure_validators. \
             err: {err:?}"
        ),
    }

    Ok(())
}
