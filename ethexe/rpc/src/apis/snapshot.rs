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

use crate::{SnapshotRpcConfig, errors};
use anyhow::{Context as _, Result, anyhow};
use dashmap::DashSet;
use ethexe_common::db::LatestDataStorageRO;
use ethexe_db::{Database, RocksDatabase};
use gprimitives::H256;
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
    core::{SubscriptionResult, async_trait},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sp_core::Bytes;
use std::{
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

const SNAPSHOT_ARCHIVE_NAME: &str = "snapshot.tar.zst";
const SNAPSHOT_CHECKPOINT_DIR_NAME: &str = "checkpoint";
const SNAPSHOT_COMPRESSION: &str = "tar.zst";
static SNAPSHOT_SERVICE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SnapshotStreamItem {
    Manifest {
        snapshot_id: String,
        block_hash: H256,
        total_bytes: u64,
        chunk_size: u64,
        total_chunks: u64,
        sha256_hex: String,
        compression: String,
    },
    Chunk {
        index: u64,
        data: Bytes,
    },
    Completed {
        total_chunks: u64,
        total_bytes: u64,
    },
}

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "snapshot"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "snapshot"))]
pub trait Snapshot {
    #[subscription(name = "download", unsubscribe = "downloadUnsubscribe", item = SnapshotStreamItem)]
    async fn download(&self) -> SubscriptionResult;
}

#[derive(Clone)]
pub struct SnapshotApi {
    service: Arc<SnapshotService>,
}

impl SnapshotApi {
    pub fn new(db: Database, rocks_db: RocksDatabase, cfg: SnapshotRpcConfig) -> Self {
        Self {
            service: Arc::new(SnapshotService::new(db, rocks_db, cfg)),
        }
    }
}

#[async_trait]
impl SnapshotServer for SnapshotApi {
    async fn download(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        let permit = self
            .service
            .concurrency_limiter
            .clone()
            .try_acquire_owned()
            .map_err(|_| errors::unavailable("too many concurrent snapshot downloads"))?;

        let service = self.service.clone();
        let snapshot = tokio::task::spawn_blocking(move || service.prepare_snapshot())
            .await
            .map_err(|err| errors::internal_with(format!("snapshot worker panicked: {err}")))?
            .map_err(|err| errors::internal_with(format!("failed to prepare snapshot: {err:#}")))?;

        let sink = match pending.accept().await {
            Ok(sink) => sink,
            Err(err) => {
                self.service
                    .clone()
                    .cleanup_snapshot_async(snapshot.work_dir)
                    .await;
                return Err(err.into());
            }
        };

        self.service
            .clone()
            .spawn_streaming_task(sink, snapshot, permit);

        Ok(())
    }
}

#[derive(Debug)]
struct SnapshotService {
    db: Database,
    rocks_db: RocksDatabase,
    chunk_size_bytes: usize,
    retention: Duration,
    service_prefix: String,
    work_root: PathBuf,
    concurrency_limiter: Arc<Semaphore>,
    active_work_dirs: DashSet<PathBuf>,
    id_counter: AtomicU64,
}

#[derive(Debug)]
struct PreparedSnapshot {
    snapshot_id: String,
    block_hash: H256,
    archive_path: PathBuf,
    work_dir: PathBuf,
    total_bytes: u64,
    chunk_size_bytes: usize,
    total_chunks: u64,
    sha256_hex: String,
}

impl SnapshotService {
    fn new(db: Database, rocks_db: RocksDatabase, cfg: SnapshotRpcConfig) -> Self {
        let chunk_size_bytes = cfg.chunk_size_bytes.max(1);
        let max_concurrent_downloads = cfg.max_concurrent_downloads.max(1);
        let service_prefix = format!(
            "svc{:x}",
            SNAPSHOT_SERVICE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        Self {
            db,
            rocks_db,
            chunk_size_bytes,
            retention: Duration::from_secs(cfg.retention_secs),
            work_root: std::env::temp_dir()
                .join("ethexe-rpc-snapshots")
                .join(&service_prefix),
            service_prefix,
            concurrency_limiter: Arc::new(Semaphore::new(max_concurrent_downloads as usize)),
            active_work_dirs: DashSet::default(),
            id_counter: AtomicU64::new(0),
        }
    }

    fn prepare_snapshot(&self) -> Result<PreparedSnapshot> {
        if let Err(err) = self.cleanup_stale_snapshots() {
            tracing::warn!("failed to cleanup stale snapshot artifacts: {err:#}");
        }

        fs::create_dir_all(&self.work_root).with_context(|| {
            format!(
                "failed to create snapshot workspace {}",
                self.work_root.display()
            )
        })?;

        let snapshot_id = self.next_snapshot_id();
        let work_dir = self.work_root.join(&snapshot_id);
        let checkpoint_dir = work_dir.join(SNAPSHOT_CHECKPOINT_DIR_NAME);
        let archive_path = work_dir.join(SNAPSHOT_ARCHIVE_NAME);

        fs::create_dir_all(&work_dir)
            .with_context(|| format!("failed to create work dir {}", work_dir.display()))?;

        self.active_work_dirs.insert(work_dir.clone());

        let prepared = (|| {
            let work_dir_for_result = work_dir.clone();
            let block_hash = self
                .db
                .latest_data()
                .ok_or_else(|| anyhow!("latest data wasn't found in database"))?
                .synced_block
                .hash;

            self.rocks_db
                .create_checkpoint(&checkpoint_dir)
                .with_context(|| {
                    format!(
                        "failed to create rocksdb checkpoint at {}",
                        checkpoint_dir.display()
                    )
                })?;

            Self::pack_checkpoint_archive(&checkpoint_dir, &archive_path)?;

            let (total_bytes, sha256_hex) = Self::compute_file_metadata(&archive_path)?;
            let total_chunks = if total_bytes == 0 {
                0
            } else {
                total_bytes.div_ceil(self.chunk_size_bytes as u64)
            };

            Ok(PreparedSnapshot {
                snapshot_id,
                block_hash,
                archive_path,
                work_dir: work_dir_for_result,
                total_bytes,
                chunk_size_bytes: self.chunk_size_bytes,
                total_chunks,
                sha256_hex,
            })
        })();

        if prepared.is_err() {
            self.cleanup_snapshot(work_dir);
        }

        prepared
    }

    fn pack_checkpoint_archive(checkpoint_dir: &Path, archive_path: &Path) -> Result<()> {
        let archive = File::create(archive_path)
            .with_context(|| format!("failed to create {}", archive_path.display()))?;

        let mut encoder =
            zstd::Encoder::new(archive, 3).context("failed to initialize zstd encoder")?;
        {
            let mut tar_builder = tar::Builder::new(&mut encoder);
            tar_builder
                .append_dir_all("rocksdb", checkpoint_dir)
                .with_context(|| {
                    format!(
                        "failed to append checkpoint directory {} to tar",
                        checkpoint_dir.display()
                    )
                })?;
            tar_builder
                .finish()
                .context("failed to finish tar archive")?;
        }

        encoder.finish().context("failed to finish zstd stream")?;

        Ok(())
    }

    fn compute_file_metadata(path: &Path) -> Result<(u64, String)> {
        let mut file = File::open(path)
            .with_context(|| format!("failed to open snapshot archive {}", path.display()))?;

        let mut hasher = Sha256::new();
        let mut total_bytes = 0u64;
        let mut buffer = [0u8; 64 * 1024];

        loop {
            let read = file
                .read(&mut buffer)
                .with_context(|| format!("failed to read archive {}", path.display()))?;

            if read == 0 {
                break;
            }

            hasher.update(&buffer[..read]);
            total_bytes += read as u64;
        }

        Ok((total_bytes, hex::encode(hasher.finalize())))
    }

    fn next_snapshot_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let id = self.id_counter.fetch_add(1, Ordering::Relaxed);

        format!("{}-{timestamp:x}-{id:x}", self.service_prefix)
    }

    fn cleanup_stale_snapshots(&self) -> Result<()> {
        let read_dir = match fs::read_dir(&self.work_root) {
            Ok(read_dir) => read_dir,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to list snapshot dir {}", self.work_root.display())
                });
            }
        };

        for entry in read_dir {
            let entry = entry.context("failed to read entry from snapshot root")?;
            let path = entry.path();
            if !path.is_dir() || self.active_work_dirs.contains(&path) {
                continue;
            }

            let modified = entry
                .metadata()
                .with_context(|| format!("failed to read metadata for {}", path.display()))?
                .modified()
                .with_context(|| format!("failed to read mtime for {}", path.display()))?;

            let age = modified.elapsed().unwrap_or_default();
            if age >= self.retention {
                fs::remove_dir_all(&path).with_context(|| {
                    format!("failed to remove stale snapshot {}", path.display())
                })?;
            }
        }

        Ok(())
    }

    fn cleanup_snapshot(&self, work_dir: PathBuf) {
        self.active_work_dirs.remove(&work_dir);
        if let Err(err) = fs::remove_dir_all(&work_dir)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                "failed to remove snapshot artifact {}: {err}",
                work_dir.display()
            );
        }
    }

    fn spawn_streaming_task(
        self: Arc<Self>,
        sink: SubscriptionSink,
        snapshot: PreparedSnapshot,
        _permit: OwnedSemaphorePermit,
    ) {
        tokio::spawn(async move {
            let work_dir = snapshot.work_dir.clone();

            let res = self.stream_snapshot(&sink, &snapshot).await;
            if let Err(err) = res {
                tracing::warn!(
                    "failed to stream snapshot {}: {err:#}",
                    snapshot.snapshot_id
                );
            }

            self.cleanup_after_streaming_async(work_dir).await;
        });
    }

    async fn cleanup_after_streaming_async(self: Arc<Self>, work_dir: PathBuf) {
        let service = self;
        if let Err(err) = tokio::task::spawn_blocking(move || {
            service.cleanup_snapshot(work_dir);
            if let Err(err) = service.cleanup_stale_snapshots() {
                tracing::warn!("failed to cleanup stale snapshots after streaming: {err:#}");
            }
        })
        .await
        {
            tracing::warn!("snapshot cleanup worker panicked: {err}");
        }
    }

    async fn cleanup_snapshot_async(self: Arc<Self>, work_dir: PathBuf) {
        let service = self;
        if let Err(err) =
            tokio::task::spawn_blocking(move || service.cleanup_snapshot(work_dir)).await
        {
            tracing::warn!("snapshot cleanup worker panicked: {err}");
        }
    }

    async fn stream_snapshot(
        &self,
        sink: &SubscriptionSink,
        snapshot: &PreparedSnapshot,
    ) -> Result<()> {
        Self::send_item(
            sink,
            SnapshotStreamItem::Manifest {
                snapshot_id: snapshot.snapshot_id.clone(),
                block_hash: snapshot.block_hash,
                total_bytes: snapshot.total_bytes,
                chunk_size: snapshot.chunk_size_bytes as u64,
                total_chunks: snapshot.total_chunks,
                sha256_hex: snapshot.sha256_hex.clone(),
                compression: SNAPSHOT_COMPRESSION.to_owned(),
            },
        )
        .await?;

        self.stream_archive_chunks(sink, snapshot).await?;

        Self::send_item(
            sink,
            SnapshotStreamItem::Completed {
                total_chunks: snapshot.total_chunks,
                total_bytes: snapshot.total_bytes,
            },
        )
        .await
    }

    async fn stream_archive_chunks(
        &self,
        sink: &SubscriptionSink,
        snapshot: &PreparedSnapshot,
    ) -> Result<()> {
        let (sender, mut receiver) = mpsc::channel::<Result<(u64, Vec<u8>), String>>(4);
        let archive_path = snapshot.archive_path.clone();
        let chunk_size_bytes = snapshot.chunk_size_bytes;

        tokio::task::spawn_blocking(move || {
            if let Err(err) = Self::read_archive_chunks(archive_path, chunk_size_bytes, &sender) {
                let _ = sender.blocking_send(Err(err.to_string()));
            }
        });

        while let Some(next) = receiver.recv().await {
            let (index, chunk) = match next {
                Ok(ok) => ok,
                Err(err) => return Err(anyhow!(err)),
            };

            Self::send_item(
                sink,
                SnapshotStreamItem::Chunk {
                    index,
                    data: chunk.into(),
                },
            )
            .await?;
        }

        Ok(())
    }

    fn read_archive_chunks(
        archive_path: PathBuf,
        chunk_size_bytes: usize,
        sender: &mpsc::Sender<Result<(u64, Vec<u8>), String>>,
    ) -> Result<()> {
        let mut file = File::open(&archive_path)
            .with_context(|| format!("failed to open archive {}", archive_path.display()))?;

        let mut index = 0u64;
        loop {
            let mut chunk = vec![0u8; chunk_size_bytes];
            let read = file
                .read(&mut chunk)
                .with_context(|| format!("failed to read archive {}", archive_path.display()))?;

            if read == 0 {
                break;
            }

            chunk.truncate(read);
            if sender.blocking_send(Ok((index, chunk))).is_err() {
                break;
            }
            index += 1;
        }

        Ok(())
    }

    async fn send_item(sink: &SubscriptionSink, item: SnapshotStreamItem) -> Result<()> {
        let message = SubscriptionMessage::from_json(&item)
            .context("failed to serialize snapshot stream item")?;

        sink.send(message)
            .await
            .context("failed to send snapshot stream item")
    }
}
