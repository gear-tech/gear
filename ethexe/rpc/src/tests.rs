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
    RpcService, SnapshotClient, SnapshotRpcConfig, SnapshotStreamItem,
};
use ethexe_common::{
    Announce, HashOf, ProtocolTimelines, SimpleBlockData,
    db::{DBConfig, DBGlobals, GlobalsStorageRO},
    ecdsa::PrivateKey,
    gear::MAX_BLOCK_GAS_LIMIT,
    injected::{AddressedInjectedTransaction, Promise, SignedPromise},
    mock::Mock,
};
use ethexe_db::{CASDatabase, Database, RawDatabase, RocksDatabase, VERSION};
use futures::StreamExt;
use gear_core::{
    message::{ReplyCode, SuccessReplyReason},
    rpc::ReplyInfo,
};
use gprimitives::H256;
use jsonrpsee::{
    server::ServerHandle,
    ws_client::{HeaderMap, HeaderValue, WsClient, WsClientBuilder},
};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File},
    io::Write as _,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};
use tempfile::TempDir;
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
        snapshot: None,
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
                .send_transaction_and_watch(AddressedInjectedTransaction::mock(()))
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

struct SnapshotFixture {
    _temp_dir: TempDir,
    rocks_db: RocksDatabase,
    db: Database,
    synced_block_hash: H256,
    sample_hash: H256,
    sample_payload: Vec<u8>,
}

impl SnapshotFixture {
    fn new(entry_count: usize, entry_size: usize) -> Self {
        Self::new_with_payload_mode(entry_count, entry_size, false)
    }

    fn new_high_entropy(entry_count: usize, entry_size: usize) -> Self {
        Self::new_with_payload_mode(entry_count, entry_size, true)
    }

    fn new_with_payload_mode(entry_count: usize, entry_size: usize, high_entropy: bool) -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let db_rocks = RocksDatabase::open(temp_dir.path().to_path_buf())
            .expect("Failed to open rocks database");
        let db_raw = RawDatabase::from_one(&db_rocks);

        db_raw.kv.set_config(DBConfig {
            version: VERSION,
            chain_id: 0,
            router_address: Default::default(),
            timelines: ProtocolTimelines::default(),
            genesis_block_hash: H256::from_low_u64_be(1),
            genesis_announce_hash: HashOf::<Announce>::zero(),
        });

        let synced_block_hash = H256::from_low_u64_be(42);
        db_raw.kv.set_globals(DBGlobals {
            start_block_hash: H256::from_low_u64_be(1),
            start_announce_hash: HashOf::<Announce>::zero(),
            latest_synced_block: SimpleBlockData {
                hash: synced_block_hash,
                header: Default::default(),
            },
            latest_prepared_block_hash: synced_block_hash,
            latest_computed_announce_hash: HashOf::<Announce>::zero(),
        });

        let db_reopened =
            Database::try_from_raw(db_raw).expect("Constructs Database from RawDatabase");

        let mut sample = None;
        for index in 0..entry_count {
            let payload = if high_entropy {
                pseudo_random_payload(index as u64 + 1, entry_size)
            } else {
                let mut payload = vec![0u8; entry_size];
                payload.fill((index % 255) as u8);
                payload
            };

            let hash = db_reopened.cas().write(&payload);
            if sample.is_none() {
                sample = Some((hash, payload.clone()));
            }
        }

        let (sample_hash, sample_payload) = sample.expect("snapshot fixture should have data");

        Self {
            _temp_dir: temp_dir,
            rocks_db: db_rocks,
            db: db_reopened,
            synced_block_hash,
            sample_hash,
            sample_payload,
        }
    }
}

fn pseudo_random_payload(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed;
    let mut payload = vec![0u8; len];

    for byte in &mut payload {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }

    payload
}

fn snapshot_artifacts_root() -> PathBuf {
    std::env::temp_dir().join("ethexe-rpc-snapshots")
}

fn unused_local_addr() -> SocketAddr {
    let listener =
        std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("should bind localhost");
    let addr = listener
        .local_addr()
        .expect("should obtain local socket address");
    drop(listener);
    addr
}

fn snapshot_rpc_config(
    token: &str,
    retention_secs: u64,
    max_concurrent_downloads: u32,
) -> SnapshotRpcConfig {
    SnapshotRpcConfig {
        auth_bearer_token: token.to_string(),
        chunk_size_bytes: 32 * 1024,
        retention_secs,
        max_concurrent_downloads,
    }
}

async fn start_snapshot_server(
    listen_addr: SocketAddr,
    fixture: &SnapshotFixture,
    snapshot_cfg: Option<SnapshotRpcConfig>,
) -> (ServerHandle, RpcService) {
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
        snapshot: snapshot_cfg,
    };
    RpcServer::new(rpc_config, fixture.db.clone())
        .with_snapshot_source(fixture.rocks_db.clone())
        .run_server()
        .await
        .expect("RPC Server will start successfully")
}

async fn snapshot_client(listen_addr: SocketAddr, token: Option<&str>) -> WsClient {
    let mut builder = WsClientBuilder::new();

    if let Some(token) = token {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("Authorization header must be valid"),
        );
        builder = builder.set_headers(headers);
    }

    builder
        .build(format!("ws://{listen_addr}"))
        .await
        .expect("WS client will be created")
}

struct DownloadedSnapshot {
    snapshot_id: String,
    block_hash: H256,
    total_bytes: u64,
    chunk_size: u64,
    total_chunks: u64,
    sha256_hex: String,
    chunks: Vec<(u64, Vec<u8>)>,
}

struct StreamedDownloadedSnapshot {
    snapshot_id: String,
    block_hash: H256,
    total_bytes: u64,
    chunk_size: u64,
    total_chunks: u64,
    sha256_hex: String,
}

async fn download_snapshot(client: &WsClient) -> DownloadedSnapshot {
    let mut subscription = client
        .download()
        .await
        .expect("Snapshot subscription must be created");

    let manifest = subscription
        .next()
        .await
        .expect("Manifest item should be present")
        .expect("Manifest item should not contain subscription error");

    let (snapshot_id, block_hash, total_bytes, chunk_size, total_chunks, sha256_hex, _compression) =
        match manifest {
            SnapshotStreamItem::Manifest {
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
            } => (
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
            ),
            other => panic!("Expected manifest item, got: {other:?}"),
        };

    let mut chunks = Vec::with_capacity(total_chunks as usize);
    for _ in 0..total_chunks {
        let item = subscription
            .next()
            .await
            .expect("Chunk item should be present")
            .expect("Chunk item should not contain subscription error");
        match item {
            SnapshotStreamItem::Chunk { index, data } => chunks.push((index, data.0)),
            other => panic!("Expected chunk item, got: {other:?}"),
        }
    }

    let completed = subscription
        .next()
        .await
        .expect("Completed item should be present")
        .expect("Completed item should not contain subscription error");

    match completed {
        SnapshotStreamItem::Completed {
            total_chunks: done_chunks,
            total_bytes: done_bytes,
        } => {
            assert_eq!(done_chunks, total_chunks);
            assert_eq!(done_bytes, total_bytes);
        }
        other => panic!("Expected completed item, got: {other:?}"),
    }

    DownloadedSnapshot {
        snapshot_id,
        block_hash,
        total_bytes,
        chunk_size,
        total_chunks,
        sha256_hex,
        chunks,
    }
}

async fn download_snapshot_to_file(
    client: &WsClient,
    archive_path: PathBuf,
) -> StreamedDownloadedSnapshot {
    let mut subscription = client
        .download()
        .await
        .expect("Snapshot subscription must be created");

    let manifest = subscription
        .next()
        .await
        .expect("Manifest item should be present")
        .expect("Manifest item should not contain subscription error");

    let (snapshot_id, block_hash, total_bytes, chunk_size, total_chunks, sha256_hex, _compression) =
        match manifest {
            SnapshotStreamItem::Manifest {
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
            } => (
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
            ),
            other => panic!("Expected manifest item, got: {other:?}"),
        };

    let mut archive = File::create(&archive_path).expect("Archive file should be created");
    let mut hasher = Sha256::new();
    let mut bytes_written = 0u64;
    let mut chunks_written = 0u64;

    for expected_index in 0..total_chunks {
        let item = subscription
            .next()
            .await
            .expect("Chunk item should be present")
            .expect("Chunk item should not contain subscription error");
        match item {
            SnapshotStreamItem::Chunk { index, data } => {
                assert_eq!(index, expected_index, "chunk index should be sequential");
                archive
                    .write_all(&data.0)
                    .expect("Chunk data should be written to archive file");
                hasher.update(&data.0);
                bytes_written += data.0.len() as u64;
                chunks_written += 1;
            }
            other => panic!("Expected chunk item, got: {other:?}"),
        }
    }

    let completed = subscription
        .next()
        .await
        .expect("Completed item should be present")
        .expect("Completed item should not contain subscription error");

    match completed {
        SnapshotStreamItem::Completed {
            total_chunks: done_chunks,
            total_bytes: done_bytes,
        } => {
            assert_eq!(done_chunks, total_chunks);
            assert_eq!(done_bytes, total_bytes);
        }
        other => panic!("Expected completed item, got: {other:?}"),
    }

    assert_eq!(bytes_written, total_bytes);
    assert_eq!(chunks_written, total_chunks);
    assert_eq!(sha256_hex, hex::encode(hasher.finalize()));

    StreamedDownloadedSnapshot {
        snapshot_id,
        block_hash,
        total_bytes,
        chunk_size,
        total_chunks,
        sha256_hex,
    }
}

fn extract_snapshot_archive(archive_path: &PathBuf, extract_dir: &PathBuf) {
    let archive = File::open(archive_path).expect("Archive file should be opened");
    let decoder = zstd::Decoder::new(archive).expect("Archive zstd decoder should be created");
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(extract_dir)
        .expect("Archive should be unpacked successfully");
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_download_success_with_valid_token() {
    let fixture = SnapshotFixture::new(32, 16 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let snapshot = download_snapshot(&client).await;

    assert_eq!(snapshot.block_hash, fixture.synced_block_hash);
    assert!(!snapshot.snapshot_id.is_empty());
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_download_rejects_missing_token() {
    let fixture = SnapshotFixture::new(16, 8 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, None).await;
    let err = client
        .download()
        .await
        .expect_err("Download must fail without auth token");
    assert!(
        err.to_string().contains("Unauthorized"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_download_rejects_invalid_token() {
    let fixture = SnapshotFixture::new(16, 8 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("wrong-token")).await;
    let err = client
        .download()
        .await
        .expect_err("Download must fail for wrong token");
    assert!(
        err.to_string().contains("Unauthorized"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_methods_not_registered_when_disabled() {
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_new_server(listen_addr).await;

    let client = snapshot_client(listen_addr, None).await;
    let err = client
        .download()
        .await
        .expect_err("Download must fail when snapshot API is disabled");
    assert!(
        err.to_string().contains("Method not found"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_methods_not_registered_without_snapshot_source() {
    let listen_addr = unused_local_addr();
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
        snapshot: Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    };
    let (_handle, _rpc) = RpcServer::new(rpc_config, Database::memory())
        .run_server()
        .await
        .expect("RPC Server will start successfully");

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let err = client
        .download()
        .await
        .expect_err("Download must fail when snapshot source is unavailable");
    assert!(
        err.to_string().contains("Method not found"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_stream_manifest_then_chunks_then_completed() {
    let fixture = SnapshotFixture::new(32, 8 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let snapshot = download_snapshot(&client).await;
    let indexes: Vec<_> = snapshot.chunks.iter().map(|(i, _)| *i).collect();
    assert_eq!(indexes, (0..snapshot.total_chunks).collect::<Vec<_>>());
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_chunk_count_and_total_bytes_match_manifest() {
    let fixture = SnapshotFixture::new(32, 8 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let snapshot = download_snapshot(&client).await;
    let bytes_downloaded: usize = snapshot.chunks.iter().map(|(_, chunk)| chunk.len()).sum();

    assert_eq!(snapshot.total_chunks as usize, snapshot.chunks.len());
    assert_eq!(snapshot.total_bytes, bytes_downloaded as u64);
    assert_eq!(snapshot.chunk_size, 32 * 1024);
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_sha256_matches_streamed_archive() {
    let fixture = SnapshotFixture::new(32, 8 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let snapshot = download_snapshot(&client).await;

    let mut hasher = Sha256::new();
    for (_, chunk) in &snapshot.chunks {
        hasher.update(chunk);
    }
    assert_eq!(snapshot.sha256_hex, hex::encode(hasher.finalize()));
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_cleanup_removes_request_artifacts_and_ttl_gc_removes_stale() {
    let fixture = SnapshotFixture::new(32, 8 * 1024);
    let listen_addr = unused_local_addr();

    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 0, 1)),
    )
    .await;

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let first_snapshot = download_snapshot(&client).await;
    let service_prefix = first_snapshot
        .snapshot_id
        .split_once('-')
        .expect("snapshot id should include service prefix")
        .0
        .to_string();
    let stale_dir = snapshot_artifacts_root()
        .join(service_prefix)
        .join("stale-artifact-for-test");
    std::fs::create_dir_all(&stale_dir).expect("stale dir should be created");

    let second_snapshot = download_snapshot(&client).await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    let snapshot_dir = snapshot_artifacts_root()
        .join(
            second_snapshot
                .snapshot_id
                .split_once('-')
                .expect("snapshot id should include service prefix")
                .0,
        )
        .join(second_snapshot.snapshot_id);
    assert!(
        !snapshot_dir.exists(),
        "snapshot directory should be removed"
    );
    assert!(!stale_dir.exists(), "stale artifact should be removed");
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn snapshot_concurrency_limit_enforced() {
    let fixture = SnapshotFixture::new(64, 512 * 1024);
    let listen_addr = unused_local_addr();
    let (_handle, _rpc) = start_snapshot_server(
        listen_addr,
        &fixture,
        Some(snapshot_rpc_config("snapshot-token", 600, 1)),
    )
    .await;

    let first_client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let second_client = snapshot_client(listen_addr, Some("snapshot-token")).await;

    let first_task = tokio::spawn(async move { first_client.download().await });
    tokio::time::sleep(Duration::from_millis(10)).await;

    let err = second_client
        .download()
        .await
        .expect_err("Second concurrent download must fail");
    let err_text = err.to_string();
    assert!(
        err_text.contains("Service unavailable") || err_text.contains("Internal error"),
        "unexpected error: {err}"
    );

    let first_subscription = first_task
        .await
        .expect("first download task should finish")
        .expect("first download should succeed");
    first_subscription
        .unsubscribe()
        .await
        .expect("unsubscribe should succeed");
}

#[tokio::test]
#[ignore = "manual large snapshot test; generates and downloads a 2+ GiB archive over WS"]
#[ntest::timeout(7_200_000)]
async fn snapshot_download_large_real_archive_2gib() {
    const ENTRY_COUNT: usize = 544;
    const ENTRY_SIZE: usize = 4 * 1024 * 1024;
    const CHUNK_SIZE_BYTES: usize = 1_048_576;

    let fixture = SnapshotFixture::new_high_entropy(ENTRY_COUNT, ENTRY_SIZE);
    let listen_addr = unused_local_addr();
    let snapshot_cfg = SnapshotRpcConfig {
        auth_bearer_token: "snapshot-token".to_string(),
        chunk_size_bytes: CHUNK_SIZE_BYTES,
        retention_secs: 600,
        max_concurrent_downloads: 1,
    };
    let (_handle, _rpc) = start_snapshot_server(listen_addr, &fixture, Some(snapshot_cfg)).await;

    let output_dir = tempfile::tempdir().expect("temporary directory should be created");
    let archive_path = output_dir.path().join("snapshot.tar.zst");
    let extract_dir = output_dir.path().join("extract");

    let client = snapshot_client(listen_addr, Some("snapshot-token")).await;
    let snapshot = download_snapshot_to_file(&client, archive_path.clone()).await;

    assert_eq!(snapshot.block_hash, fixture.synced_block_hash);
    assert_eq!(snapshot.chunk_size, CHUNK_SIZE_BYTES as u64);
    assert!(snapshot.total_bytes > 2 * 1024 * 1024 * 1024);
    assert_eq!(
        snapshot.total_chunks,
        snapshot.total_bytes.div_ceil(snapshot.chunk_size)
    );
    assert!(!snapshot.snapshot_id.is_empty());
    assert!(!snapshot.sha256_hex.is_empty());

    fs::create_dir_all(&extract_dir).expect("extract directory should be created");
    extract_snapshot_archive(&archive_path, &extract_dir);

    let reopened_db = RocksDatabase::open(extract_dir.join("rocksdb"))
        .expect("Extracted RocksDB checkpoint should reopen successfully");
    let reopened_database = RawDatabase::from_one(&reopened_db);
    let reopened_database = Database::try_from_raw(reopened_database)
        .expect("Database should be constructed from RawDatabase successfully");
    let block_hash = {
        let globals_db = reopened_database.globals();

        globals_db.latest_synced_block.hash
    };
    assert_eq!(block_hash, fixture.synced_block_hash);
    assert_eq!(
        reopened_db.read(fixture.sample_hash),
        Some(fixture.sample_payload.clone())
    );
}
