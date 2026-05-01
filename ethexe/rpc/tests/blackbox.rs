// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use ethexe_common::{
    HashOf,
    db::{CodesStorageRW, InjectedStorageRW},
    ecdsa::PrivateKey,
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance, Promise,
        SignedInjectedTransaction, SignedPromise,
    },
    mock::Mock,
};
use ethexe_db::Database;
use ethexe_rpc::{RpcConfig, RpcEvent, RpcServer, RpcService};
use futures::StreamExt;
use gear_core::{
    code::{InstantiatedSectionSizes, InstrumentedCode},
    message::{ReplyCode, SuccessReplyReason},
    rpc::ReplyInfo,
};
use gprimitives::H256;
use jsonrpsee::{
    core::client::{ClientT, SubscriptionClientT},
    http_client::{HttpClient, HttpClientBuilder},
    rpc_params,
    server::ServerHandle,
    ws_client::{WsClient, WsClientBuilder},
};
use parity_scale_codec::Encode;
use proptest::{
    collection,
    prelude::{Just, Strategy, any},
    prop_assert_eq,
    test_runner::{Config as ProptestConfig, FileFailurePersistence, TestRunner},
};
use sp_core::Bytes;
use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener},
    time::Duration,
};
use tokio::task::JoinHandle;

struct BlackBoxRpc {
    addr: SocketAddr,
    handle: ServerHandle,
    service: Option<RpcService>,
    service_task: Option<JoinHandle<()>>,
}

impl BlackBoxRpc {
    async fn start() -> Self {
        Self::start_with_db(Database::memory()).await
    }

    async fn start_with_db(db: Database) -> Self {
        let addr = unused_local_addr();
        let config = RpcConfig {
            listen_addr: addr,
            cors: None,
            gas_allowance: MAX_BLOCK_GAS_LIMIT,
            chunk_size: 2,
            with_dev_api: false,
        };

        let (handle, service) = RpcServer::new(config, db)
            .run_server()
            .await
            .expect("RPC server must start");

        Self {
            addr,
            handle,
            service: Some(service),
            service_task: None,
        }
    }

    fn http_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn ws_url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    fn spawn_accepting_service(&mut self) {
        let mut service = self
            .service
            .take()
            .expect("service can only be spawned once");

        self.service_task = Some(tokio::spawn(async move {
            while let Some(event) = service.next().await {
                let RpcEvent::InjectedTransaction {
                    transaction,
                    response_sender,
                } = event;

                response_sender
                    .send(InjectedTransactionAcceptance::Accept)
                    .expect("RPC response receiver must be alive");

                let promise = promise_for(transaction);

                // The RPC method registers its promise waiter after the service accepts
                // the transaction, so publish from the next task turn.
                tokio::time::sleep(Duration::from_millis(10)).await;
                service.provide_promise(promise);
            }
        }));
    }

    fn stop(self) {
        self.handle.stop().expect("RPC server must stop");
        if let Some(service_task) = self.service_task {
            service_task.abort();
        }
    }
}

fn unused_local_addr() -> SocketAddr {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("ephemeral localhost port must be available");
    listener
        .local_addr()
        .expect("ephemeral localhost address must be available")
}

fn promise_for(transaction: AddressedInjectedTransaction) -> SignedPromise {
    let promise = Promise {
        tx_hash: transaction.tx.data().to_hash(),
        reply: ReplyInfo {
            payload: Vec::new(),
            value: 0,
            code: ReplyCode::Success(SuccessReplyReason::Manual),
        },
    };

    SignedPromise::create(PrivateKey::random(), promise).expect("promise signing must succeed")
}

async fn http_client(rpc: &BlackBoxRpc) -> HttpClient {
    HttpClientBuilder::default()
        .build(rpc.http_url())
        .expect("HTTP client must connect")
}

async fn ws_client(rpc: &BlackBoxRpc) -> WsClient {
    WsClientBuilder::default()
        .build(rpc.ws_url())
        .await
        .expect("WS client must connect")
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn original_code_request_returns_stored_code_to_http_client() {
    let db = Database::memory();
    let code = [0, b'a', b's', b'm', 1, 0, 0, 0];
    let code_id = db.set_original_code(&code);
    let rpc = BlackBoxRpc::start_with_db(db).await;
    let client = http_client(&rpc).await;

    let returned_code = client
        .request::<Bytes, _>("code_getOriginal", rpc_params![H256::from(code_id)])
        .await
        .expect("stored original code must be returned");

    assert_eq!(returned_code.0, code.to_vec().encode());

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn instrumented_code_request_returns_stored_code_to_http_client() {
    let db = Database::memory();
    let runtime_id = 7;
    let code_id = db.set_original_code(b"original");
    let instrumented = InstrumentedCode::new(
        vec![1, 2, 3, 4],
        InstantiatedSectionSizes::new(10, 20, 30, 40, 50, 60),
    );
    db.set_instrumented_code(runtime_id, code_id, instrumented.clone());

    let rpc = BlackBoxRpc::start_with_db(db).await;
    let client = http_client(&rpc).await;

    let returned_code = client
        .request::<Bytes, _>(
            "code_getInstrumented",
            rpc_params![runtime_id, H256::from(code_id)],
        )
        .await
        .expect("stored instrumented code must be returned");

    assert_eq!(returned_code.0, instrumented.encode());

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn missing_code_request_returns_public_json_rpc_error() {
    let rpc = BlackBoxRpc::start().await;
    let client = http_client(&rpc).await;

    let error = client
        .request::<Vec<u8>, _>("code_getOriginal", rpc_params![H256::zero()])
        .await
        .expect_err("missing code must be reported as a JSON-RPC error");

    assert!(
        error
            .to_string()
            .contains("Failed to get code by supplied id"),
        "unexpected error: {error}"
    );

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn send_transaction_returns_acceptance_to_http_client() {
    let mut rpc = BlackBoxRpc::start().await;
    rpc.spawn_accepting_service();

    let client = http_client(&rpc).await;
    let acceptance = client
        .request::<InjectedTransactionAcceptance, _>(
            "injected_sendTransaction",
            rpc_params![AddressedInjectedTransaction::mock(())],
        )
        .await
        .expect("accepted transaction must return a client-visible response");

    assert_eq!(acceptance, InjectedTransactionAcceptance::Accept);

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn send_transaction_and_watch_yields_promise_to_ws_client() {
    let mut rpc = BlackBoxRpc::start().await;
    rpc.spawn_accepting_service();

    let client = ws_client(&rpc).await;
    let transaction = AddressedInjectedTransaction::mock(());
    let expected_tx_hash = transaction.tx.data().to_hash();

    let mut subscription = client
        .subscribe::<SignedPromise, _>(
            "injected_sendTransactionAndWatch",
            rpc_params![transaction],
            "injected_sendTransactionAndWatchUnsubscribe",
        )
        .await
        .expect("subscription must be created");

    let promise = subscription
        .next()
        .await
        .expect("subscription must yield a promise")
        .expect("promise item must be valid");

    assert_eq!(promise.data().tx_hash, expected_tx_hash);
    assert_eq!(
        promise.data().reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn get_transactions_returns_stored_signed_transaction_to_http_client() {
    let db = Database::memory();
    let transaction = AddressedInjectedTransaction::mock(()).tx;
    let tx_hash = transaction.data().to_hash();
    db.set_injected_transaction(transaction.clone());

    let rpc = BlackBoxRpc::start_with_db(db).await;
    let client = http_client(&rpc).await;

    let transactions = client
        .request::<Vec<Option<SignedInjectedTransaction>>, _>(
            "injected_getTransactions",
            rpc_params![vec![tx_hash]],
        )
        .await
        .expect("stored injected transaction must be returned");

    assert_eq!(transactions, vec![Some(transaction)]);

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn get_transactions_preserves_missing_entries_to_http_client() {
    let rpc = BlackBoxRpc::start().await;
    let client = http_client(&rpc).await;
    let missing_hash = unsafe { HashOf::<InjectedTransaction>::new(H256::repeat_byte(1)) };

    let transactions = client
        .request::<Vec<Option<SignedInjectedTransaction>>, _>(
            "injected_getTransactions",
            rpc_params![vec![missing_hash]],
        )
        .await
        .expect("missing transaction lookup must return a successful response");

    assert_eq!(transactions, vec![None]);

    rpc.stop();
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn get_transactions_preserves_requested_order_hits_misses_and_duplicates() {
    let handle = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let mut runner = TestRunner::new(ProptestConfig {
            cases: 16,
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        });

        let strategy = collection::vec(any::<bool>(), 0..=32);

        runner
            .run(&strategy, |is_stored_flags| {
                handle.block_on(async {
                    let db = Database::memory();
                    let stored_transactions: Vec<_> = (0..3)
                        .map(|_| AddressedInjectedTransaction::mock(()).tx)
                        .collect();
                    let stored_hashes: Vec<_> = stored_transactions
                        .iter()
                        .map(|transaction| transaction.data().to_hash())
                        .collect();

                    for transaction in &stored_transactions {
                        db.set_injected_transaction(transaction.clone());
                    }

                    let missing_hashes = [
                        unsafe { HashOf::<InjectedTransaction>::new(H256::repeat_byte(1)) },
                        unsafe { HashOf::<InjectedTransaction>::new(H256::repeat_byte(2)) },
                        unsafe { HashOf::<InjectedTransaction>::new(H256::repeat_byte(3)) },
                    ];

                    let requested_hashes: Vec<_> = is_stored_flags
                        .iter()
                        .enumerate()
                        .map(|(index, is_stored)| {
                            if *is_stored {
                                stored_hashes[index % stored_hashes.len()]
                            } else {
                                missing_hashes[index % missing_hashes.len()]
                            }
                        })
                        .collect();

                    let expected: Vec<_> = is_stored_flags
                        .iter()
                        .enumerate()
                        .map(|(index, is_stored)| {
                            is_stored.then(|| {
                                stored_transactions[index % stored_transactions.len()].clone()
                            })
                        })
                        .collect();

                    let rpc = BlackBoxRpc::start_with_db(db).await;
                    let client = http_client(&rpc).await;

                    let transactions = client
                        .request::<Vec<Option<SignedInjectedTransaction>>, _>(
                            "injected_getTransactions",
                            rpc_params![requested_hashes],
                        )
                        .await
                        .expect("mixed transaction lookup must return a successful response");

                    rpc.stop();

                    prop_assert_eq!(transactions, expected);

                    Ok(())
                })
            })
            .expect("generated transaction lookup cases must satisfy the public RPC contract");
    })
    .await
    .expect("blocking proptest runner must not panic");
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn get_transactions_accepts_up_to_public_limit() {
    let handle = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let mut runner = TestRunner::new(ProptestConfig {
            cases: 8,
            failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
            ..ProptestConfig::default()
        });

        let missing_hash = unsafe { HashOf::<InjectedTransaction>::new(H256::repeat_byte(4)) };
        let strategy = (Just(missing_hash), 0usize..=100).prop_map(|(hash, len)| vec![hash; len]);

        runner
            .run(&strategy, |requested_hashes| {
                handle.block_on(async {
                    let expected = vec![None::<SignedInjectedTransaction>; requested_hashes.len()];
                    let rpc = BlackBoxRpc::start().await;
                    let client = http_client(&rpc).await;

                    let transactions = client
                        .request::<Vec<Option<SignedInjectedTransaction>>, _>(
                            "injected_getTransactions",
                            rpc_params![requested_hashes],
                        )
                        .await
                        .expect(
                            "requests at or below the public transaction lookup limit must succeed",
                        );

                    rpc.stop();

                    prop_assert_eq!(transactions, expected);

                    Ok(())
                })
            })
            .expect(
                "generated in-limit transaction lookup cases must satisfy the public RPC contract",
            );
    })
    .await
    .expect("blocking proptest runner must not panic");
}
