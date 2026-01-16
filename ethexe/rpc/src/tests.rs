// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{
    InjectedApi, InjectedClient, InjectedTransactionAcceptance, RpcConfig, RpcEvent, RpcServer,
    RpcService,
};

use ethexe_common::{
    Address,
    ecdsa::{PrivateKey, SignedMessage},
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{InjectedTransaction, Promise, RpcOrNetworkInjectedTx, SignedPromise},
    mock::Mock,
};
use ethexe_db::Database;
use ethexe_processor::RunnerConfig;
use futures::StreamExt;
use gear_core::{
    message::{ReplyCode, SuccessReplyReason},
    rpc::ReplyInfo,
};
use jsonrpsee::{
    server::ServerHandle,
    ws_client::{WsClient, WsClientBuilder},
};
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
                        let promises = tx_batch.drain(..).map(Self::create_promise).collect();
                        self.rpc.provide_promises(promises);
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

    fn create_promise(tx: RpcOrNetworkInjectedTx) -> SignedPromise {
        let tx = tx.tx.into_data();
        let promise = Promise {
            tx_hash: tx.to_hash(),
            reply: ReplyInfo {
                payload: vec![],
                // Take value from the transaction for testing purposes.
                value: tx.value,
                code: ReplyCode::Success(SuccessReplyReason::Manual),
            },
        };
        SignedPromise::create(PrivateKey::random(), promise).expect("Signing promise will succeed")
    }
}

/// Starts a new RPC server listening on the given address.
async fn start_new_server(listen_addr: SocketAddr) -> (ServerHandle, RpcService) {
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        runner_config: RunnerConfig::common(2, MAX_BLOCK_GAS_LIMIT),
    };
    RpcServer::new(rpc_config, Database::memory())
        .run_server()
        .await
        .expect("RPC Server will start successfully")
}

/// This helper function waits until all promise subscriptions being closed and cleaned up.
async fn wait_for_closed_subscriptions(injected_api: InjectedApi) {
    while injected_api.promise_subscribers_count() > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Creates a new WebSocket client connected to the given address.
async fn ws_client(addr: SocketAddr) -> WsClient {
    WsClientBuilder::new()
        .build(format!("ws://{addr}"))
        .await
        .expect("WS client will be created")
}

#[tokio::test]
#[ntest::timeout(20_000)]
async fn test_cleanup_promise_subscribers() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8002);
    let service = MockService::new(listen_addr).await;
    let injected_api = service.injected_api();

    // Spawn the mock service main loop.
    let _handle = service.spawn();

    let ws_client = ws_client(listen_addr).await;

    // Correct workflow: send transaction, receive promise, unsubscribe.
    {
        let mut subscribers = JoinSet::new();
        for _ in 0..20 {
            let mut sub = ws_client
                .send_transaction_and_watch(RpcOrNetworkInjectedTx::mock(()))
                .await
                .expect("Subscription will be created");

            subscribers.spawn(async move {
                let promise = sub
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");

                assert_eq!(
                    promise.data().reply.code,
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
                .send_transaction_and_watch(RpcOrNetworkInjectedTx::mock(()))
                .await
                .expect("Subscription will be created");

            subscribers.spawn(async move {
                let promise = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");

                assert_eq!(
                    promise.data().reply.code,
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
                .send_transaction_and_watch(RpcOrNetworkInjectedTx::mock(()))
                .await
                .expect("Subscription will be created");
            subscriptions.push(subscription);
        }

        drop(subscriptions);

        wait_for_closed_subscriptions(injected_api.clone()).await;
    }
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn test_concurrent_multiple_clients() {
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8010);
    let service = MockService::new(listen_addr).await;
    let injected_api = service.injected_api();

    // Spawn the mock service main loop.
    let _handle = service.spawn();

    let mut tasks = JoinSet::new();
    for _ in 0..10 {
        tasks.spawn(async move {
            let client = ws_client(listen_addr).await;
            // Each client sequentially creates 50 subscriptions.
            let mut subscriptions = vec![];
            for _ in 0..50 {
                let mut subscription = client
                    .send_transaction_and_watch(RpcOrNetworkInjectedTx::mock(()))
                    .await
                    .expect("Subscription will be created");

                let promise = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");

                assert_eq!(
                    promise.data().reply.code,
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
#[ntest::timeout(20_000)]
async fn test_subscribe_all_promises() {
    tracing_subscriber::fmt().init();

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8020);
    let service = MockService::new(listen_addr).await;
    let _injected_api = service.injected_api();

    // Spawn the mock service main loop.
    let _handle = service.spawn();

    let transactions_count = 100u32;
    let subscribers = 5;

    let mut tasks = JoinSet::new();
    for _ in 0..subscribers {
        let client = ws_client(listen_addr).await;

        tasks.spawn(async move {
            let mut subscription = client
                .subscribe_promises()
                .await
                .expect("Successfully established subscription");

            for expected_idx in 0..transactions_count {
                let promise = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result");

                assert_eq!(
                    promise.data().reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );
                assert_eq!(
                    promise.data().reply.value,
                    expected_idx as u128,
                    "Ordering of the promises should be preserved"
                );
            }
        });
    }

    let client = ws_client(listen_addr).await;

    for idx in 0..transactions_count {
        let mut tx = InjectedTransaction::mock(());
        tx.value = idx as u128;
        let tx = RpcOrNetworkInjectedTx {
            recipient: Address::default(),
            tx: SignedMessage::create(PrivateKey::random(), tx).unwrap(),
        };

        let _ = client
            .send_transaction(tx)
            .await
            .expect("Transaction will be sent");
    }

    let _ = tasks.join_all().await;
}
