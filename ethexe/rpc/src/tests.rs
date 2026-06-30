// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    CodeClient, InjectedApi, InjectedClient, InjectedTransactionAcceptance, RpcConfig, RpcEvent,
    RpcServer, RpcService, test_utils::wasm_with_custom_section,
};
use ethexe_common::{
    SignedMessage, ValidatorsVec,
    db::{CodesStorageRW, OnChainStorageRW},
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

        let (handle, rpc) = start_new_server(listen_addr, db).await;
        Self {
            rpc,
            handle,
            validator_key,
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

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_same_transaction_multiple_and_late_watchers() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8014);
    let MockService {
        mut rpc,
        handle,
        validator_key,
    } = MockService::new(listen_addr).await;

    // Inject promises/receipts through a clone of the API; the server holds the
    // same Arc-shared manager, so dispatch reaches the registered subscribers.
    // rpc.injected_api is accessible here because tests is a submodule of lib.rs.
    let injected_api = rpc.injected_api.clone();

    // Manual acceptance pump: answers `Accept` only, never produces promises.
    let pump = tokio::spawn(async move {
        while let Some(RpcEvent::InjectedTransaction {
            response_sender, ..
        }) = rpc.next().await
        {
            let _ = response_sender.send(InjectedTransactionAcceptance::Accept);
        }
    });

    let first_client = WsClientBuilder::new()
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("first WS client will be created");
    let second_client = WsClientBuilder::new()
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("second WS client will be created");

    let tx = mock_signed_transaction();
    let tx_hash = tx.data().to_hash();

    // Two concurrent watchers for the SAME transaction. Each blocks until the
    // pump accepts its relayed tx; both then register as Pending (no receipt yet).
    let (first, second) = tokio::join!(
        first_client.send_transaction_and_watch(tx.clone()),
        second_client.send_transaction_and_watch(tx.clone()),
    );
    let mut first = first.expect("first subscription will be created");
    let mut second = second.expect("second subscription will be created");

    // Deterministic proof of two *active* pending subscribers before any receipt.
    assert_eq!(injected_api.subscribers_count(), 2);

    // Exactly one promise + one receipt, fanned out to both subscribers.
    let promise = Promise::mock(tx_hash);
    let receipt =
        SignedMessage::create(validator_key, Receipt::Promise(promise.to_compact())).unwrap();
    injected_api.on_computed_promise(promise.clone());
    injected_api.on_tx_receipt(receipt.into());

    let first_receipt = first.next().await.expect("first item").expect("decodes");
    let second_receipt = second.next().await.expect("second item").expect("decodes");
    assert_eq!(first_receipt.data(), &Receipt::Promise(promise.clone()));
    assert_eq!(second_receipt.data(), &Receipt::Promise(promise.clone()));

    wait_for_closed_subscriptions(injected_api.clone()).await;

    // Kill the acceptance pump BEFORE the late watcher so the "Ready path does not
    // re-relay" guarantee is actually exercised: with no acceptor, an accidental
    // relay errors/hangs instead of being silently accepted. Aborting drops the
    // pump's captured `rpc` (and thus the relay `mpsc::Receiver`); awaiting the
    // join handle ensures that drop has completed before the call below, so a
    // regression that relays hits a closed channel rather than racing the drop.
    pump.abort();
    let _ = pump.await;

    // Late watcher for the same tx: the receipt is now stored, so it must be served
    // from the cached `Ready` path immediately, without relaying.
    let mut late = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        first_client.send_transaction_and_watch(tx.clone()),
    )
    .await
    .expect("late subscription must not block on a relay")
    .expect("late subscription will be created");
    let late_receipt = tokio::time::timeout(std::time::Duration::from_secs(5), late.next())
        .await
        .expect("cached receipt must arrive without a relay")
        .expect("late item")
        .expect("decodes");
    assert_eq!(late_receipt.data(), &Receipt::Promise(promise));

    handle.stop().expect("RPC server must stop");
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_relay_failure_cleans_pending_subscriber() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8015);
    let MockService { rpc, handle, .. } = MockService::new(listen_addr).await;
    let injected_api = rpc.injected_api.clone();
    // Dropping `rpc` closes the relay receiver; the ERROR log from relay.rs is expected here.
    drop(rpc);

    let client = WsClientBuilder::new()
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("WS client will be created");

    let result = client
        .send_transaction_and_watch(mock_signed_transaction())
        .await;

    assert!(
        result.is_err(),
        "relay must fail after its receiver is dropped"
    );
    assert_eq!(injected_api.subscribers_count(), 0);
    handle.stop().expect("RPC server must stop");
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn test_rejection_keeps_other_same_transaction_watcher() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8016);
    let MockService {
        mut rpc,
        handle,
        validator_key,
    } = MockService::new(listen_addr).await;
    let injected_api = rpc.injected_api.clone();

    let pump = tokio::spawn(async move {
        let RpcEvent::InjectedTransaction {
            response_sender, ..
        } = rpc.next().await.expect("first relay event");
        response_sender
            .send(InjectedTransactionAcceptance::Accept)
            .expect("first response receiver remains open");

        let RpcEvent::InjectedTransaction {
            response_sender, ..
        } = tokio::time::timeout(std::time::Duration::from_secs(1), rpc.next())
            .await
            .expect("second same-tx watcher must reach the relayer")
            .expect("second relay event");
        response_sender
            .send(InjectedTransactionAcceptance::Reject {
                reason: "rejected by test".into(),
            })
            .expect("second response receiver remains open");
    });

    let first_client = WsClientBuilder::new()
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("first WS client will be created");
    let second_client = WsClientBuilder::new()
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("second WS client will be created");
    let tx = mock_signed_transaction();
    let tx_hash = tx.data().to_hash();

    let (first, second) = tokio::join!(
        first_client.send_transaction_and_watch(tx.clone()),
        second_client.send_transaction_and_watch(tx),
    );
    let mut accepted = match (first, second) {
        (Ok(subscription), Err(_)) | (Err(_), Ok(subscription)) => subscription,
        _ => panic!("exactly one watcher must be accepted"),
    };
    pump.await.expect("acceptance pump must finish");
    assert_eq!(injected_api.subscribers_count(), 1);

    let promise = Promise::mock(tx_hash);
    let receipt =
        SignedMessage::create(validator_key, Receipt::Promise(promise.to_compact())).unwrap();
    injected_api.on_computed_promise(promise.clone());
    injected_api.on_tx_receipt(receipt.into());

    let delivered = accepted.next().await.expect("item").expect("decodes");
    assert_eq!(delivered.data(), &Receipt::Promise(promise));
    wait_for_closed_subscriptions(injected_api).await;
    handle.stop().expect("RPC server must stop");
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
