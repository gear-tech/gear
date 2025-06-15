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

use alloy::{
    consensus::{SidecarCoder, SimpleCoder, Transaction},
    eips::eip4844::kzg_to_versioned_hash,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::beacon::sidecar::BeaconBlobBundle,
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{CodesStorageRead, OnChainStorageRead},
    CodeBlobInfo,
};
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{CodeId, H256};
use reqwest::Client;
use std::{collections::HashSet, fmt, hash::RandomState, pin::Pin, task::Poll};
use tokio::time::{self, Duration};

pub mod local;

#[derive(Clone, PartialEq, Eq)]
pub struct BlobData {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub code: Vec<u8>,
}

impl fmt::Debug for BlobData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BlobData")
            .field("code_id", &self.code_id)
            .field("timestamp", &self.timestamp)
            .field("code", &format_args!("{} bytes", self.code.len()))
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum BlobLoaderEvent {
    BlobLoaded(BlobData),
}

// TODO (#4674): write tests for BlobLoaderService implementations
pub trait BlobLoaderService:
    Stream<Item = Result<BlobLoaderEvent>> + FusedStream + Send + Unpin
{
    fn load_codes(&mut self, codes: HashSet<CodeId>, attempts: Option<u8>) -> Result<()>;

    fn into_box(self) -> Box<dyn BlobLoaderService>;

    fn pending_codes_len(&self) -> usize;
}

impl fmt::Debug for BlobLoaderEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlobLoaderEvent::BlobLoaded(data) => data.fmt(f),
        }
    }
}

#[derive(Clone)]
pub struct ConsensusLayerConfig {
    pub ethereum_rpc: String,
    pub ethereum_beacon_rpc: String,
    pub beacon_block_time: Duration,
}

#[derive(Clone)]
struct ConsensusLayerBlobReader {
    pub provider: RootProvider,
    pub http_client: Client,
    pub config: ConsensusLayerConfig,
}

impl ConsensusLayerBlobReader {
    async fn read_code_from_tx_hash(
        self,
        expected_code_id: CodeId,
        timestamp: u64,
        tx_hash: H256,
        attempts: Option<u8>,
    ) -> Result<BlobData> {
        let code = self
            .read_blob_from_tx_hash(tx_hash, attempts)
            .await
            .map_err(|err| anyhow!("failed to read blob: {err}"))?;

        if CodeId::generate(&code) != expected_code_id {
            return Err(anyhow!("unexpected code id"));
        }

        Ok(BlobData {
            code_id: expected_code_id,
            timestamp,
            code,
        })
    }

    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        //TODO: read genesis from `{ethereum_beacon_rpc}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1695902400;

        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or_else(|| anyhow!("failed to get transaction"))?;

        let blob_versioned_hashes = tx
            .blob_versioned_hashes()
            .ok_or_else(|| anyhow!("failed to get versioned hashes"))?;
        let blob_versioned_hashes = HashSet::<_, RandomState>::from_iter(blob_versioned_hashes);
        let block_hash = tx
            .block_hash
            .ok_or_else(|| anyhow!("failed to get block hash"))?;
        let block = self
            .provider
            .get_block_by_hash(block_hash)
            .await?
            .ok_or_else(|| anyhow!("failed to get block"))?;
        let slot = (block.header.timestamp - BEACON_GENESIS_BLOCK_TIME)
            / self.config.beacon_block_time.as_secs();
        let blob_bundle_result = match attempts {
            Some(attempts) => {
                let mut count = 0;
                loop {
                    log::trace!("trying to get blob, attempt #{}", count + 1);
                    let blob_bundle_result = self.read_blob_bundle(slot).await;
                    if blob_bundle_result.is_ok() || count >= attempts {
                        break blob_bundle_result;
                    } else {
                        time::sleep(self.config.beacon_block_time).await;
                        count += 1;
                    }
                }
            }
            None => self.read_blob_bundle(slot).await,
        };
        let blob_bundle = blob_bundle_result?;

        let mut blobs = Vec::with_capacity(blob_versioned_hashes.len());
        for blob_data in blob_bundle.into_iter().filter(|blob_data| {
            blob_versioned_hashes
                .contains(&kzg_to_versioned_hash(blob_data.kzg_commitment.as_ref()))
        }) {
            blobs.push(*blob_data.blob);
        }

        let mut coder = SimpleCoder::default();
        let data = coder
            .decode_all(&blobs)
            .ok_or_else(|| anyhow!("failed to decode blobs"))?
            .concat();

        Ok(data)
    }

    async fn read_blob_bundle(&self, slot: u64) -> reqwest::Result<BeaconBlobBundle> {
        let ethereum_beacon_rpc = &self.config.ethereum_beacon_rpc;
        self.http_client
            .get(format!(
                "{ethereum_beacon_rpc}/eth/v1/beacon/blob_sidecars/{slot}"
            ))
            .send()
            .await?
            .json::<BeaconBlobBundle>()
            .await
    }
}

pub trait Database: CodesStorageRead + OnChainStorageRead + Unpin + Send + Clone + 'static {}
impl<T: CodesStorageRead + OnChainStorageRead + Unpin + Send + Clone + 'static> Database for T {}

pub struct BlobLoader<DB: Database> {
    futures: FuturesUnordered<BoxFuture<'static, Result<BlobData>>>,
    codes_loading: HashSet<CodeId>,

    blobs_reader: ConsensusLayerBlobReader,
    db: DB,
}

impl<DB: Database> Stream for BlobLoader<DB> {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let future = self.futures.poll_next_unpin(cx);
        match future {
            Poll::Ready(Some(result)) => match result {
                Ok(blob_data) => {
                    let code_id = &blob_data.code_id;
                    self.codes_loading.remove(code_id);
                    Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(blob_data))))
                }
                Err(e) => Poll::Ready(Some(Err(e))),
            },
            _ => Poll::Pending,
        }
    }
}

impl<DB: Database> FusedStream for BlobLoader<DB> {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<DB: Database> BlobLoader<DB> {
    pub async fn new(db: DB, consensus_cfg: ConsensusLayerConfig) -> Result<Self> {
        Ok(Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),

            blobs_reader: ConsensusLayerBlobReader {
                provider: ProviderBuilder::default()
                    .connect(&consensus_cfg.ethereum_rpc)
                    .await?,
                http_client: Client::new(),
                config: consensus_cfg,
            },
            db,
        })
    }
}

impl<DB: Database> BlobLoaderService for BlobLoader<DB> {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.codes_loading.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>, attempts: Option<u8>) -> Result<()> {
        for code_id in codes {
            if self.codes_loading.contains(&code_id) {
                continue;
            }

            let CodeBlobInfo { timestamp, tx_hash } = self
                .db
                .code_blob_info(code_id)
                .ok_or(anyhow!("not found code info for {code_id} in db"))?;

            if let Some(code) = self.db.original_code(code_id) {
                log::warn!("Code {code_id} is already loaded, skipping loading from remote source");
                self.futures.push(
                    futures::future::ready(Ok(BlobData {
                        code_id,
                        timestamp,
                        code,
                    }))
                    .boxed(),
                );
                continue;
            }

            self.codes_loading.insert(code_id);
            self.futures.push(
                self.blobs_reader
                    .clone()
                    .read_code_from_tx_hash(code_id, timestamp, tx_hash, attempts)
                    .boxed(),
            );
        }

        Ok(())
    }
}
