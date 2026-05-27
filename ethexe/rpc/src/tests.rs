// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    InjectedApi, InjectedClient, InjectedTransactionAcceptance, RpcConfig, RpcEvent, RpcServer,
    RpcService,
};
use ethexe_common::{
    SignedMessage,
    ecdsa::PrivateKey,
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{AddressedInjectedTransaction, Promise, Receipt, SignedCompactTxReceipt},
    mock::Mock,
};
use ethexe_db::Database;
use futures::StreamExt;
use gear_core::message::{ReplyCode, SuccessReplyReason};
use jsonrpsee::{server::ServerHandle, ws_client::WsClientBuilder};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::task::{JoinHandle, JoinSet};

/// [`MockService`] simulates main `ethexe_service::Service` behavior.
/// It accepts injected transactions and periodically runs batches of them and produces promises.
struct MockService {
    rpc: RpcService,
    handle: ServerHandle,
}

impl MockService {
    /// Creates a new mock service which runs an RPC server listening on the given address.
    pub async fn new(listen_addr: SocketAddr) -> Self {
        let (handle, rpc) = start_new_server(listen_addr).await;
        Self { rpc, handle }
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
                            let (promise, receipt) = Self::create_promise_for(tx);
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

    fn create_promise_for(tx: AddressedInjectedTransaction) -> (Promise, SignedCompactTxReceipt) {
        let promise = Promise::mock(tx.tx.data().to_hash());
        let receipt =
            SignedMessage::create(PrivateKey::random(), Receipt::Promise(promise.to_compact()))
                .unwrap();
        (promise, receipt.into())
    }
}

/// Starts a new RPC server listening on the given address.
async fn start_new_server(listen_addr: SocketAddr) -> (ServerHandle, RpcService) {
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
        with_dev_api: false,
    };
    RpcServer::new(rpc_config, Database::memory())
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
                .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
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
                .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
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
                .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
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
                    .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
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
