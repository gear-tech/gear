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
use anyhow::Result;
use ethexe_common::{
    ecdsa::PrivateKey,
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{AddressedInjectedTransaction, Promise, SignedPromise},
    mock::Mock,
};
use ethexe_db::Database;
use futures::StreamExt;
use gear_core::{
    message::{ReplyCode, SuccessReplyReason},
    rpc::ReplyInfo,
};
use jsonrpsee::{
    core::client::Error as JsonRpcError,
    server::ServerHandle,
    types::{ErrorCode},
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
                        let promises = tx_batch.drain(..).map(Self::create_promise_for).collect();
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

    fn create_promise_for(tx: AddressedInjectedTransaction) -> SignedPromise {
        let promise = Promise {
            tx_hash: tx.tx.data().to_hash(),
            reply: ReplyInfo {
                payload: vec![],
                value: 0,
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
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
    };
    RpcServer::new(rpc_config, Database::memory())
        .run_server()
        .await
        .expect("RPC Server will start successfully")
}

async fn new_ws_client(url: impl AsRef<str>) -> Result<WsClient> {
    WsClientBuilder::new().build(url).await.map_err(Into::into)
}

/// This helper function waits until all promise subscriptions being closed and cleaned up.
async fn wait_for_closed_subscriptions(injected_api: InjectedApi) {
    while injected_api.promise_subscribers_count() > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn try_init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

#[tokio::test]
#[ntest::timeout(20_000)]
async fn test_cleanup_promise_subscribers() {
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
                let promise = sub
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result")
                    .unwrap_promise();

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
                .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
                .await
                .expect("Subscription will be created");

            subscribers.spawn(async move {
                let promise = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result")
                    .unwrap_promise();

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

                let promise = subscription
                    .next()
                    .await
                    .expect("Promise will be received")
                    .expect("No error in subscription result")
                    .unwrap_promise();

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
async fn test_rpc_server_errors() {
    try_init_logger();

    const INVALID_TRANSACTION: &str = "
      {
        \"recipient\": \"0x0000000000000000000000000000000000000000\",
        \"tx\": {
          \"data\": {
            \"destination\": \"0x1566de93e5a0e3baf567239e95030110feabd8df\",
            \"payload\": \"0x\",
            \"value\": 10,
            \"reference_block\": \"0x457e2af4d7be721fc74da0034d6e5cc23dd8c41971302230e7e1f6d3234d14e3\",
            \"salt\": \"0x5fb82ec819dc15fe6ec1ed7d5d6ad59eb8b7db4ca584896fc8b83db1c5f515c9\"
          },
          \"signature\": \"0x22f7b3223625cfea5a5c05d0dcbed5492ebf2c16f87266c92c68e83cb67e2be80e9dbe25cee00c7c17425e21c753e081c827e2c69546ba676afe9e72c7786ae41c\",
          \"address\": \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\"
        }
      }
    ";

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8033);
    let (_handle, _service) = start_new_server(listen_addr).await;

    let ws_addr = format!("ws://{listen_addr}");
    let client = new_ws_client(ws_addr).await.expect("connection to server");

    let transaction =
        serde_json::from_str(INVALID_TRANSACTION).expect("successfully deserialize from string");
    let error = client
        .send_transaction_and_watch(transaction)
        .await
        .unwrap_err();
    println!("error from rpc: {error:?}");

    let JsonRpcError::Call(error) = error else {
        panic!("error")
    };
    println!("code: {}", error.code());
    assert_eq!(error.code(), ErrorCode::InvalidParams.code());
}
