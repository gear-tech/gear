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

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::Parser;
use ethexe_common::{
    Announce, HashOf, ProtocolTimelines, SimpleBlockData,
    db::{DBConfig, DBGlobals, GlobalsStorageRO},
    gear::MAX_BLOCK_GAS_LIMIT,
};
use ethexe_db::{CASDatabase, Database, RawDatabase, RocksDatabase, VERSION};
use ethexe_rpc::{
    RpcConfig, RpcServer, RpcService, SnapshotClient, SnapshotRpcConfig, SnapshotStreamItem,
};
use gprimitives::H256;
use jsonrpsee::{
    server::ServerHandle,
    ws_client::{HeaderMap, HeaderValue, WsClient, WsClientBuilder},
};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File},
    io::{Cursor, Write as _},
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    str::FromStr,
};
use tempfile::TempDir;

#[derive(Debug, Parser)]
struct Cli {
    /// Existing snapshot-enabled RPC endpoint to verify instead of spawning a local fixture server.
    #[arg(long)]
    ws_url: Option<String>,

    /// Snapshot cases in the form `entry_count x entry_size_bytes`.
    #[arg(long = "case", value_name = "ENTRY_COUNTxENTRY_SIZE_BYTES")]
    cases: Vec<String>,

    /// Bearer token used to authorize snapshot downloads.
    #[arg(long, default_value = "snapshot-token")]
    token: String,

    /// Chunk size configured on the snapshot RPC server.
    #[arg(long, default_value_t = 32 * 1024)]
    chunk_bytes: usize,

    /// Snapshot retention configured on the snapshot RPC server.
    #[arg(long, default_value_t = 600)]
    retention_secs: u64,

    /// Maximum concurrent downloads configured on the snapshot RPC server.
    #[arg(long, default_value_t = 1)]
    max_concurrent: u32,

    /// Optional directory to persist downloaded archives and extracted checkpoints.
    #[arg(long)]
    output_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct SnapshotCase {
    entry_count: usize,
    entry_size: usize,
}

impl SnapshotCase {
    fn label(&self) -> String {
        format!("{}x{}", self.entry_count, self.entry_size)
    }
}

impl FromStr for SnapshotCase {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let (entry_count, entry_size) = value.split_once('x').ok_or_else(|| {
            anyhow!("invalid case `{value}`, expected format ENTRY_COUNTxENTRY_SIZE_BYTES")
        })?;

        let entry_count = entry_count
            .parse()
            .with_context(|| format!("invalid entry count in case `{value}`"))?;
        let entry_size = entry_size
            .parse()
            .with_context(|| format!("invalid entry size in case `{value}`"))?;

        Ok(Self {
            entry_count,
            entry_size,
        })
    }
}

#[derive(Debug)]
struct SnapshotFixture {
    _temp_dir: TempDir,
    rocks_db: RocksDatabase,
    db: Database,
    expected_block_hash: H256,
    sample_hash: H256,
    sample_payload: Vec<u8>,
}

#[derive(Debug)]
struct DownloadedSnapshot {
    snapshot_id: String,
    block_hash: H256,
    total_bytes: u64,
    chunk_size: u64,
    total_chunks: u64,
    sha256_hex: String,
    compression: String,
    archive_bytes: Vec<u8>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(output_dir) = &cli.output_dir {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;
    }

    if cli.ws_url.is_some() {
        ensure!(
            cli.cases.is_empty(),
            "`--case` is only supported when the verifier spawns local fixture servers"
        );
        return run_external_snapshot_check(&cli).await;
    }

    let cases = parse_cases(&cli)?;
    for case in cases {
        run_case(&cli, &case).await?;
    }

    Ok(())
}

fn parse_cases(cli: &Cli) -> Result<Vec<SnapshotCase>> {
    if cli.cases.is_empty() {
        return Ok(vec![
            SnapshotCase {
                entry_count: 8,
                entry_size: 1024,
            },
            SnapshotCase {
                entry_count: 64,
                entry_size: 32 * 1024,
            },
            SnapshotCase {
                entry_count: 128,
                entry_size: 128 * 1024,
            },
        ]);
    }

    cli.cases.iter().map(|case| case.parse()).collect()
}

async fn run_case(cli: &Cli, case: &SnapshotCase) -> Result<()> {
    let fixture = SnapshotFixture::new(case)?;
    let listen_addr = unused_local_addr()?;
    let snapshot_config = SnapshotRpcConfig {
        auth_bearer_token: cli.token.clone(),
        chunk_size_bytes: cli.chunk_bytes.max(1),
        retention_secs: cli.retention_secs,
        max_concurrent_downloads: cli.max_concurrent.max(1),
    };
    let (handle, _rpc) = start_snapshot_server(listen_addr, &fixture, snapshot_config).await?;

    let result = async {
        let client = snapshot_client(format!("ws://{listen_addr}"), &cli.token).await?;
        let downloaded = download_snapshot(&client).await?;
        verify_downloaded_snapshot(&downloaded, &fixture, cli.output_dir.as_deref(), case)?;

        println!(
            "verified case {}: bytes={}, chunks={}, sha256={}",
            case.label(),
            downloaded.total_bytes,
            downloaded.total_chunks,
            downloaded.sha256_hex
        );

        Ok(())
    }
    .await;

    handle
        .stop()
        .map_err(|err| anyhow!("failed to stop snapshot rpc server: {err}"))?;
    handle.stopped().await;

    result
}

async fn run_external_snapshot_check(cli: &Cli) -> Result<()> {
    let ws_url = cli
        .ws_url
        .as_ref()
        .expect("checked above that external mode is enabled");
    let client = snapshot_client(ws_url.clone(), &cli.token).await?;
    let downloaded = download_snapshot(&client).await?;
    verify_external_snapshot(&downloaded, cli.output_dir.as_deref())?;

    println!(
        "verified external snapshot: bytes={}, chunks={}, sha256={}, block_hash={}",
        downloaded.total_bytes,
        downloaded.total_chunks,
        downloaded.sha256_hex,
        downloaded.block_hash
    );

    Ok(())
}

impl SnapshotFixture {
    fn new(case: &SnapshotCase) -> Result<Self> {
        let temp_dir = tempfile::tempdir().context("failed to create temporary directory")?;
        let rocks_db = RocksDatabase::open(temp_dir.path().to_path_buf())
            .context("SnapshotFixture: failed to open rocks database")?;
        let db_raw = RawDatabase::from_one(&rocks_db);

        db_raw.kv.set_config(DBConfig {
            version: VERSION,
            chain_id: 0,
            router_address: Default::default(),
            timelines: ProtocolTimelines::default(),
            genesis_block_hash: H256::from_low_u64_be(1),
            genesis_announce_hash: HashOf::<Announce>::zero(),
        });

        let expected_block_hash = H256::from_low_u64_be(42);
        db_raw.kv.set_globals(DBGlobals {
            start_block_hash: H256::from_low_u64_be(1),
            start_announce_hash: HashOf::<Announce>::zero(),
            latest_synced_block: SimpleBlockData {
                hash: expected_block_hash,
                header: Default::default(),
            },
            latest_prepared_block_hash: expected_block_hash,
            latest_computed_announce_hash: HashOf::<Announce>::zero(),
        });

        let db = Database::try_from_raw(db_raw)
            .context("SnapshotFixture: failed to construct Database from RawDatabase")?;

        let mut sample = None;
        for index in 0..case.entry_count {
            let payload = pseudo_random_payload(index as u64 + 1, case.entry_size);
            let hash = db.cas().write(&payload);
            if sample.is_none() {
                sample = Some((hash, payload));
            }
        }

        let (sample_hash, sample_payload) =
            sample.ok_or_else(|| anyhow!("snapshot case must contain at least one entry"))?;

        Ok(Self {
            _temp_dir: temp_dir,
            rocks_db,
            db,
            expected_block_hash,
            sample_hash,
            sample_payload,
        })
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

async fn start_snapshot_server(
    listen_addr: SocketAddr,
    fixture: &SnapshotFixture,
    snapshot_config: SnapshotRpcConfig,
) -> Result<(ServerHandle, RpcService)> {
    let rpc_config = RpcConfig {
        listen_addr,
        cors: None,
        gas_allowance: MAX_BLOCK_GAS_LIMIT,
        chunk_size: 2,
        snapshot: Some(snapshot_config),
    };

    RpcServer::new(rpc_config, fixture.db.clone())
        .with_snapshot_source(fixture.rocks_db.clone())
        .run_server()
        .await
        .context("failed to start snapshot rpc server")
}

async fn snapshot_client(ws_url: String, token: &str) -> Result<WsClient> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}"))
            .context("failed to construct authorization header")?,
    );

    WsClientBuilder::new()
        .set_headers(headers)
        .build(ws_url)
        .await
        .context("failed to create snapshot ws client")
}

async fn download_snapshot(client: &WsClient) -> Result<DownloadedSnapshot> {
    let mut subscription = client
        .download()
        .await
        .context("failed to create snapshot subscription")?;

    let manifest = subscription
        .next()
        .await
        .ok_or_else(|| anyhow!("snapshot stream ended before manifest"))?
        .context("failed to receive manifest item")?;

    let (snapshot_id, block_hash, total_bytes, chunk_size, total_chunks, sha256_hex, compression) =
        match manifest {
            SnapshotStreamItem::Manifest {
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
                ..
            } => (
                snapshot_id,
                block_hash,
                total_bytes,
                chunk_size,
                total_chunks,
                sha256_hex,
                compression,
            ),
            other => bail!("expected manifest item, got {other:?}"),
        };

    let mut archive_bytes = Vec::with_capacity(total_bytes as usize);
    let mut received_chunks = 0u64;

    for expected_index in 0..total_chunks {
        let item = subscription
            .next()
            .await
            .ok_or_else(|| anyhow!("snapshot stream ended before chunk {expected_index}"))?
            .context("failed to receive chunk item")?;
        match item {
            SnapshotStreamItem::Chunk { index, data } => {
                ensure!(
                    index == expected_index,
                    "unexpected chunk index: expected {expected_index}, got {index}"
                );
                archive_bytes.extend_from_slice(&data.0);
                received_chunks += 1;
            }
            other => bail!("expected chunk item, got {other:?}"),
        }
    }

    let completed = subscription
        .next()
        .await
        .ok_or_else(|| anyhow!("snapshot stream ended before completion"))?
        .context("failed to receive completion item")?;

    match completed {
        SnapshotStreamItem::Completed {
            total_chunks: completed_chunks,
            total_bytes: completed_bytes,
        } => {
            ensure!(
                completed_chunks == total_chunks,
                "completed chunk count mismatch: expected {total_chunks}, got {completed_chunks}"
            );
            ensure!(
                completed_bytes == total_bytes,
                "completed byte count mismatch: expected {total_bytes}, got {completed_bytes}"
            );
        }
        other => bail!("expected completed item, got {other:?}"),
    }

    ensure!(
        received_chunks == total_chunks,
        "received chunk count mismatch: expected {total_chunks}, got {received_chunks}"
    );

    Ok(DownloadedSnapshot {
        snapshot_id,
        block_hash,
        total_bytes,
        chunk_size,
        total_chunks,
        sha256_hex,
        compression,
        archive_bytes,
    })
}

fn verify_downloaded_snapshot(
    downloaded: &DownloadedSnapshot,
    fixture: &SnapshotFixture,
    output_dir: Option<&Path>,
    case: &SnapshotCase,
) -> Result<()> {
    ensure!(
        downloaded.compression == "tar.zst",
        "unexpected compression: {}",
        downloaded.compression
    );
    ensure!(
        downloaded.block_hash == fixture.expected_block_hash,
        "unexpected manifest block hash: expected {}, got {}",
        fixture.expected_block_hash,
        downloaded.block_hash
    );
    ensure!(
        downloaded.total_bytes == downloaded.archive_bytes.len() as u64,
        "archive byte count mismatch: manifest={}, actual={}",
        downloaded.total_bytes,
        downloaded.archive_bytes.len()
    );

    let expected_chunk_count = if downloaded.total_bytes == 0 {
        0
    } else {
        downloaded.total_bytes.div_ceil(downloaded.chunk_size)
    };
    ensure!(
        downloaded.total_chunks == expected_chunk_count,
        "chunk count mismatch: manifest={}, expected={expected_chunk_count}",
        downloaded.total_chunks
    );

    let mut hasher = Sha256::new();
    hasher.update(&downloaded.archive_bytes);
    ensure!(
        downloaded.sha256_hex == hex::encode(hasher.finalize()),
        "sha256 mismatch for downloaded archive"
    );

    let artifact_dir_guard = match output_dir {
        Some(output_dir) => ArtifactDir::persistent(output_dir.join(case.label()))?,
        None => ArtifactDir::temporary()?,
    };
    let archive_path = artifact_dir_guard.path().join("snapshot.tar.zst");
    File::create(&archive_path)
        .with_context(|| format!("failed to create archive {}", archive_path.display()))?
        .write_all(&downloaded.archive_bytes)
        .with_context(|| format!("failed to write archive {}", archive_path.display()))?;

    let extracted_dir = artifact_dir_guard.path().join("extracted");
    if extracted_dir.exists() {
        fs::remove_dir_all(&extracted_dir)
            .with_context(|| format!("failed to clean extract dir {}", extracted_dir.display()))?;
    }
    fs::create_dir_all(&extracted_dir)
        .with_context(|| format!("failed to create extract dir {}", extracted_dir.display()))?;
    extract_snapshot_archive(&downloaded.archive_bytes, &extracted_dir)?;

    let reopened_db = RocksDatabase::open(extracted_dir.join("rocksdb"))
        .context("failed to reopen extracted rocksdb checkpoint")?;
    let reopened_database = RawDatabase::from_one(&reopened_db);
    let reopened_database = Database::try_from_raw(reopened_database)
        .context("failed to construct Database from RawDatabase")?;
    let block_hash = {
        let globals_db = reopened_database.globals();

        globals_db.latest_synced_block.hash
    };
    ensure!(
        block_hash == fixture.expected_block_hash,
        "unexpected synced block hash after extraction"
    );
    ensure!(
        reopened_db.read(fixture.sample_hash) == Some(fixture.sample_payload.clone()),
        "sample payload missing from reopened checkpoint"
    );

    println!(
        "  artifact={} snapshot_id={}",
        archive_path.display(),
        downloaded.snapshot_id
    );

    Ok(())
}

fn verify_external_snapshot(
    downloaded: &DownloadedSnapshot,
    output_dir: Option<&Path>,
) -> Result<()> {
    ensure!(
        downloaded.compression == "tar.zst",
        "unexpected compression: {}",
        downloaded.compression
    );
    ensure!(
        downloaded.total_bytes == downloaded.archive_bytes.len() as u64,
        "archive byte count mismatch: manifest={}, actual={}",
        downloaded.total_bytes,
        downloaded.archive_bytes.len()
    );

    let expected_chunk_count = if downloaded.total_bytes == 0 {
        0
    } else {
        downloaded.total_bytes.div_ceil(downloaded.chunk_size)
    };
    ensure!(
        downloaded.total_chunks == expected_chunk_count,
        "chunk count mismatch: manifest={}, expected={expected_chunk_count}",
        downloaded.total_chunks
    );

    let mut hasher = Sha256::new();
    hasher.update(&downloaded.archive_bytes);
    ensure!(
        downloaded.sha256_hex == hex::encode(hasher.finalize()),
        "sha256 mismatch for downloaded archive"
    );

    let artifact_dir_guard = match output_dir {
        Some(output_dir) => ArtifactDir::persistent(output_dir.join("external"))?,
        None => ArtifactDir::temporary()?,
    };
    let archive_path = artifact_dir_guard.path().join("snapshot.tar.zst");
    File::create(&archive_path)
        .with_context(|| format!("failed to create archive {}", archive_path.display()))?
        .write_all(&downloaded.archive_bytes)
        .with_context(|| format!("failed to write archive {}", archive_path.display()))?;

    let extracted_dir = artifact_dir_guard.path().join("extracted");
    if extracted_dir.exists() {
        fs::remove_dir_all(&extracted_dir)
            .with_context(|| format!("failed to clean extract dir {}", extracted_dir.display()))?;
    }
    fs::create_dir_all(&extracted_dir)
        .with_context(|| format!("failed to create extract dir {}", extracted_dir.display()))?;
    extract_snapshot_archive(&downloaded.archive_bytes, &extracted_dir)?;

    let reopened_db = RocksDatabase::open(extracted_dir.join("rocksdb"))
        .context("failed to reopen extracted rocksdb checkpoint")?;
    let reopened_database = RawDatabase::from_one(&reopened_db);
    let reopened_database = Database::try_from_raw(reopened_database)
        .context("failed to construct Database from RawDatabase")?;
    let block_hash = {
        let globals_db = reopened_database.globals();

        globals_db.latest_synced_block.hash
    };
    ensure!(
        block_hash == downloaded.block_hash,
        "manifest block hash does not match extracted checkpoint latest data"
    );

    println!(
        "  artifact={} snapshot_id={}",
        archive_path.display(),
        downloaded.snapshot_id
    );

    Ok(())
}

fn extract_snapshot_archive(archive_bytes: &[u8], extract_dir: &Path) -> Result<()> {
    let decoder = zstd::Decoder::new(Cursor::new(archive_bytes))
        .context("failed to create zstd decoder for snapshot archive")?;
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(extract_dir)
        .with_context(|| format!("failed to unpack archive into {}", extract_dir.display()))
}

fn unused_local_addr() -> Result<SocketAddr> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("failed to bind ephemeral localhost port")?;
    let addr = listener
        .local_addr()
        .context("failed to read ephemeral localhost address")?;
    drop(listener);
    Ok(addr)
}

enum ArtifactDir {
    Persistent(PathBuf),
    Temporary(TempDir),
}

impl ArtifactDir {
    fn persistent(path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create artifact dir {}", path.display()))?;
        Ok(Self::Persistent(path))
    }

    fn temporary() -> Result<Self> {
        tempfile::tempdir()
            .context("failed to create temporary artifact dir")
            .map(Self::Temporary)
    }

    fn path(&self) -> &Path {
        match self {
            Self::Persistent(path) => path,
            Self::Temporary(dir) => dir.path(),
        }
    }
}
