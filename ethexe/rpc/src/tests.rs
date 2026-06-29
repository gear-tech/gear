// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    CodeClient, InjectedApi, InjectedClient, InjectedTransactionAcceptance, PromiseEnvelope,
    PromiseSubscriptionFilter, ReplyCodeFilter, RpcConfig, RpcEvent, RpcServer, RpcService,
    test_utils::wasm_with_custom_section,
};
use ethexe_common::{
    SignedMessage, ValidatorsVec,
    db::{CodesStorageRW, InjectedStorageRW, OnChainStorageRW},
    ecdsa::{PrivateKey, PublicKey},
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{
        InjectedTransaction, Promise, Receipt, SignedCompactTxReceipt, SignedInjectedTransaction,
    },
    mock::Mock,
};
use ethexe_db::Database;
use futures::StreamExt;
use gear_core::message::{ReplyCode, SuccessReplyReason};
use jsonrpsee::{core::ClientError, server::ServerHandle, ws_client::WsClientBuilder};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::task::{JoinHandle, JoinSet};

/// [`MockService`] simulates main `ethexe_service::Service` behavior.
/// It accepts injected transactions and periodically runs batches of them and produces promises.
struct MockService {
    rpc: RpcService,
    handle: ServerHandle,
    validator_key: PrivateKey,
    db: Database,
}

impl MockService {
    /// Creates a new mock service which runs an RPC server listening on the given address.
    pub async fn new(listen_addr: SocketAddr) -> Self {
        let db = Database::memory();
        let validator_key = PrivateKey::random();
        let validator_address = PublicKey::from(&validator_key).to_address();
        db.set_validators(
            0,
            ValidatorsVec::try_from(vec![validator_address])
                .expect("test validator set must be non-empty"),
        );

        let (handle, rpc) = start_new_server(listen_addr, db.clone()).await;
        Self {
            rpc,
            handle,
            validator_key,
            db,
        }
    }

    pub fn injected_api(&self) -> InjectedApi {
        self.rpc.injected_api.clone()
    }

    /// Spawns the main loop which collects injected transactions within time intervals and
    /// then processes them in batches.
    pub fn spawn(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut tx_batch_interval =
                tokio::time::interval(std::time::Duration::from_millis(350));

            let mut tx_batch = Vec::new();

            loop {
                tokio::select! {
                    _ = tx_batch_interval.tick() => {
                        for tx in tx_batch.drain(..) {
                            let (promise, receipt) = self.create_promise_for(tx);
                            self.rpc.receive_computed_promise(promise);
                            self.rpc.receive_tx_receipt(receipt);
                        }
                    },
                    _ = self.handle.clone().stopped() => {
                        unreachable!("RPC server should not be stopped during the test")
                    },
                    event = self.rpc.next() => {
                        let RpcEvent::InjectedTransaction {transaction, response_sender} = event.expect("RPC event will be valid");

                        response_sender.send(InjectedTransactionAcceptance::Accept).expect("Response sender will be valid");
                        // Store the transaction so on_computed_promise can enrich it into a PromiseEnvelope.
                        self.db.set_injected_transaction(transaction.clone());
                        tx_batch.push(transaction);
                    },
                }
            }
        })
    }

    fn create_promise_for(
        &self,
        tx: SignedInjectedTransaction,
    ) -> (Promise, SignedCompactTxReceipt) {
        let promise = Promise::mock(tx.data().to_hash());
        let receipt = SignedMessage::create(
            self.validator_key.clone(),
            Receipt::Promise(promise.to_compact()),
        )
        .unwrap();
        (promise, receipt.into())
    }
}

/// Starts a new RPC server listening on the given address.
async fn start_new_server(listen_addr: SocketAddr, db: Database) -> (ServerHandle, RpcService) {
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
        with_dev_api: false,
    };
    RpcServer::new(rpc_config, db)
        .run_server()
        .await
        .expect("RPC Server will start successfully")
}

/// This helper function waits until all promise subscriptions being closed and cleaned up.
async fn wait_for_closed_subscriptions(injected_api: InjectedApi) {
    while injected_api.subscribers_count() > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn mock_signed_transaction() -> SignedInjectedTransaction {
    SignedMessage::create(PrivateKey::random(), InjectedTransaction::mock(())).unwrap()
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_code_read_wasm_custom_section_via_rpc() {
    const SECTION_NAME: &str = "sails:idl";

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8011);
    let db = Database::memory();
    let section_data = b"wire idl";
    let code_id = gprimitives::H256::from(
        db.set_original_code(&wasm_with_custom_section(SECTION_NAME, section_data))
            .into_bytes(),
    );
    let mut malformed_wasm = wasm_with_custom_section(SECTION_NAME, b"malformed");
    malformed_wasm.extend_from_slice(b"trailing junk");
    let malformed_code_id =
        gprimitives::H256::from(db.set_original_code(&malformed_wasm).into_bytes());

    let (handle, _rpc) = start_new_server(listen_addr, db).await;
    let ws_client = WsClientBuilder::new()
        .build(format!("ws://{}", listen_addr))
        .await
        .expect("WS client will be created");

    let result = ws_client
        .read_wasm_custom_section(code_id, SECTION_NAME.to_string())
        .await
        .expect("custom section read must succeed");

    assert_eq!(result, Some(sp_core::Bytes(section_data.to_vec())));

    let missing_section = ws_client
        .read_wasm_custom_section(code_id, "missing".to_string())
        .await
        .expect("missing section must not be an error");
    assert_eq!(missing_section, None);

    let unknown_code = ws_client
        .read_wasm_custom_section(gprimitives::H256::zero(), SECTION_NAME.to_string())
        .await
        .expect("unknown code must not be an error");
    assert_eq!(unknown_code, None);

    let err = ws_client
        .read_wasm_custom_section(malformed_code_id, SECTION_NAME.to_string())
        .await
        .expect_err("malformed stored wasm must be an RPC error");
    let ClientError::Call(err) = err else {
        panic!("expected RPC call error for malformed Wasm");
    };
    assert_eq!(err.code(), 8000);

    handle.stop().expect("RPC server must stop");
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_cleanup_promise_subscribers() {
    let _ = tracing_subscriber::fmt::try_init();

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8002);
    let service = MockService::new(listen_addr).await;
    let injected_api = service.injected_api();

    // Spawn the mock service main loop.
    let _handle = service.spawn();

    let ws_client = WsClientBuilder::new()
        .build(format!("ws://{}", listen_addr))
        .await
        .expect("WS client will be created");

    // Correct workflow: send transaction, receive promise, unsubscribe.
    {
        let mut subscribers = JoinSet::new();
        for _ in 0..20 {
            let mut sub = ws_client
                .send_transaction_and_watch(mock_signed_transaction())
                .await
                .expect("Subscription will be created");

            subscribers.spawn(async move {
                let receipt = sub
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");
                let promise = receipt.data().clone().unwrap_promise();

                assert_eq!(
                    promise.reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );

                sub.unsubscribe().await.expect("Unsubscribe will succeed");
            });
        }
        let _ = subscribers.join_all().await;
        wait_for_closed_subscriptions(injected_api.clone()).await;
    }

    // Subscribers that do not unsubscribe after receiving the promise.
    {
        let mut subscribers = JoinSet::new();
        for _ in 0..20 {
            let mut subscription = ws_client
                .send_transaction_and_watch(mock_signed_transaction())
                .await
                .expect("Subscription will be created");

            subscribers.spawn(async move {
                let receipt = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");
                let promise = receipt.data().clone().unwrap_promise();

                assert_eq!(
                    promise.reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );
            });
        }
        let _ = subscribers.join_all().await;

        wait_for_closed_subscriptions(injected_api.clone()).await;
    }

    // Subscribers that are dropped immediately after creation.
    {
        let mut subscriptions = vec![];
        for _ in 0..20 {
            let subscription = ws_client
                .send_transaction_and_watch(mock_signed_transaction())
                .await
                .expect("Subscription will be created");
            subscriptions.push(subscription);
        }

        drop(subscriptions);

        wait_for_closed_subscriptions(injected_api.clone()).await;
    }
}

// Setup worker-threads=4 to simulate concurrent clients.
#[tokio::test]
#[ntest::timeout(120_000)]
async fn test_concurrent_multiple_clients() {
    let _ = tracing_subscriber::fmt::try_init();

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8010);
    let service = MockService::new(listen_addr).await;
    let injected_api = service.injected_api();

    // Spawn the mock service main loop.
    let _handle = service.spawn();

    let mut tasks = JoinSet::new();
    for _ in 0..10 {
        tasks.spawn(async move {
            let client = WsClientBuilder::new()
                .build(format!("ws://{listen_addr}"))
                .await
                .expect("WS client will be created");

            // Each client sequentially creates 50 subscriptions.
            let mut subscriptions = vec![];
            for _ in 0..50 {
                let mut subscription = client
                    .send_transaction_and_watch(mock_signed_transaction())
                    .await
                    .expect("Subscription will be created");

                let receipt = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");
                let promise = receipt.data().clone().unwrap_promise();

                assert_eq!(
                    promise.reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );

                subscriptions.push(subscription);
            }
        });
    }

    let _ = tasks.join_all().await;
    wait_for_closed_subscriptions(injected_api).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_subscribe_promises_receives_computed_promise() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8012);
    let service = MockService::new(listen_addr).await;

    let ws_client = WsClientBuilder::new()
        .build(format!("ws://{}", listen_addr))
        .await
        .expect("WS client will be created");

    let mut sub = ws_client
        .subscribe_promises(None)
        .await
        .expect("subscription must be created");

    // Store the transaction so on_computed_promise enriches it into a PromiseEnvelope.
    let signed_tx = mock_signed_transaction();
    let expected_sender = signed_tx.address();
    let expected_destination = signed_tx.data().destination;
    let tx_hash = signed_tx.data().to_hash();
    service.db.set_injected_transaction(signed_tx);

    let promise = Promise::mock(tx_hash);
    service.rpc.receive_computed_promise(promise.clone());

    let received: PromiseEnvelope =
        tokio::time::timeout(std::time::Duration::from_secs(1), sub.next())
            .await
            .expect("promise should arrive before timeout")
            .expect("subscription item should exist")
            .expect("subscription item should decode");

    assert_eq!(received.promise, promise);
    assert_eq!(received.sender, expected_sender);
    assert_eq!(received.destination, expected_destination);

    service.handle.stop().expect("RPC server must stop");
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_subscribe_promises_applies_reply_code_filter() {
    use gear_core::rpc::ReplyInfo;

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8013);
    let service = MockService::new(listen_addr).await;

    let ws_client = WsClientBuilder::new()
        .build(format!("ws://{}", listen_addr))
        .await
        .expect("WS client will be created");

    let mut sub = ws_client
        .subscribe_promises(Some(
            PromiseSubscriptionFilter::new().reply_code(ReplyCodeFilter::success()),
        ))
        .await
        .expect("subscription must be created");

    // Non-matching promise: Unsupported reply code — store tx so the filter (not missing-tx) decides.
    let skipped_tx = mock_signed_transaction();
    let skipped_tx_hash = skipped_tx.data().to_hash();
    service.db.set_injected_transaction(skipped_tx);
    let skipped = Promise {
        tx_hash: skipped_tx_hash,
        reply: ReplyInfo {
            payload: vec![],
            value: 0,
            code: ReplyCode::Unsupported,
        },
    };
    service.rpc.receive_computed_promise(skipped);

    // Matching promise: `Promise::mock` yields `Success(Manual)`.
    let expected_tx = mock_signed_transaction();
    let expected_tx_hash = expected_tx.data().to_hash();
    service.db.set_injected_transaction(expected_tx);
    let expected = Promise::mock(expected_tx_hash);
    service.rpc.receive_computed_promise(expected.clone());

    let received: PromiseEnvelope =
        tokio::time::timeout(std::time::Duration::from_secs(1), sub.next())
            .await
            .expect("promise should arrive before timeout")
            .expect("subscription item should exist")
            .expect("subscription item should decode");

    assert_eq!(received.promise, expected);

    service.handle.stop().expect("RPC server must stop");
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_subscribe_promises_applies_sender_and_destination_filter() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8014);
    let service = MockService::new(listen_addr).await;

    let ws_client = WsClientBuilder::new()
        .build(format!("ws://{}", listen_addr))
        .await
        .expect("WS client will be created");

    // Use an explicit destination distinct from the mock default ([0u8;32]) so the
    // destination filter is actually exercised, not just the sender filter.
    // With the `ethexe` serde feature, ActorId serializes as an H160 (last 20 bytes),
    // so constructing via H160 ensures the value survives the filter JSON round-trip.
    let expected_key = PrivateKey::random();
    let expected_sender = PublicKey::from(&expected_key).to_address();
    let expected_destination = gprimitives::ActorId::from(gprimitives::H160::from([1u8; 20]));
    let expected_tx = SignedMessage::create(
        expected_key,
        InjectedTransaction {
            destination: expected_destination,
            ..InjectedTransaction::mock(())
        },
    )
    .unwrap();
    let expected_tx_hash = expected_tx.data().to_hash();
    service.db.set_injected_transaction(expected_tx);

    // A non-matching transaction: different sender and destination.
    let other_tx = mock_signed_transaction();
    let other_tx_hash = other_tx.data().to_hash();
    service.db.set_injected_transaction(other_tx);

    // Subscribe filtering on sender and destination.
    // The filter JSON serializes each field as a scalar (not array), exercising
    // FilterSet<T> custom serde across the full WS round-trip.
    let mut sub = ws_client
        .subscribe_promises(Some(
            PromiseSubscriptionFilter::new()
                .sender(expected_sender)
                .destination(expected_destination),
        ))
        .await
        .expect("subscription must be created");

    // Non-matching promise arrives first; the subscription should not yield it.
    service
        .rpc
        .receive_computed_promise(Promise::mock(other_tx_hash));
    // Matching promise arrives second.
    let expected = Promise::mock(expected_tx_hash);
    service.rpc.receive_computed_promise(expected.clone());

    let received: PromiseEnvelope =
        tokio::time::timeout(std::time::Duration::from_secs(1), sub.next())
            .await
            .expect("promise should arrive before timeout")
            .expect("subscription item should exist")
            .expect("subscription item should decode");

    assert_eq!(received.promise, expected);
    assert_eq!(received.sender, expected_sender);
    assert_eq!(received.destination, expected_destination);

    service.handle.stop().expect("RPC server must stop");
}
