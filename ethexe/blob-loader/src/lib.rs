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
use ethexe_common::{
    CodeAndIdUnchecked, CodeBlobInfo,
    db::{CodesStorageRead, OnChainStorageRead},
};
use futures::{
    FutureExt, Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
};
use gprimitives::{CodeId, H256};
use reqwest::Client;
use std::{collections::HashSet, fmt, hash::RandomState, pin::Pin, task::Poll};
use tokio::time::{self, Duration};

pub mod local;

#[derive(Clone, PartialEq, Eq)]
pub enum BlobLoaderEvent {
    BlobLoaded(CodeAndIdUnchecked),
}

#[derive(thiserror::Error, Debug)]
pub enum BlobLoaderError {
    // `ConsensusLayerBlobReader` errors
    #[error("transport error: {0}")]
    Transport(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
    #[error("failed to found transaction by hash: {0}")]
    TransactionNotFound(H256),
    #[error("failed to get blob versioned hashes from transaction: {0}")]
    BlobVersionedHashesNotFound(H256),
    #[error("failed to get transaction block hash: {0}")]
    TransactionBlockNotFound(H256),
    #[error("failed to get block by hash: {0}")]
    BlockNotFound(H256),
    #[error("failed to read blob bundle: {0}")]
    ReadBlob(#[from] reqwest::Error),
    #[error("failed to decode blobs")]
    DecodeBlobs,
    #[error("expect code id {expected_code_id}, but got {code_id}, code: {code:?}")]
    ReadUnexpectedCode {
        code: Vec<u8>,
        code_id: CodeId,
        expected_code_id: CodeId,
    },

    // `BlobLoader` errors
    #[error("failed to get code blob info for: {0}")]
    CodeBlobInfoNotFound(CodeId),

    // `LocalBlobLoader` errors
    #[error("failed to get code from local storage: {0}")]
    LocalCodeNotFound(CodeId),
}

type Result<T> = std::result::Result<T, BlobLoaderError>;

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
    /// Note: if `attempts` is `None`, it will be trying to read blob only once.
    async fn read_code_from_tx_hash(
        self,
        code_id: CodeId,
        tx_hash: H256,
        attempts: Option<u8>,
    ) -> Result<CodeAndIdUnchecked> {
        let code = self.read_blob_from_tx_hash(tx_hash, attempts).await?;

        let code_and_id = CodeAndIdUnchecked { code, code_id };

        Ok(code_and_id)
    }

    async fn read_blob_from_tx_hash(&self, tx_hash: H256, attempts: Option<u8>) -> Result<Vec<u8>> {
        //TODO: read genesis from `{ethereum_beacon_rpc}/eth/v1/beacon/genesis` with caching into some static
        const BEACON_GENESIS_BLOCK_TIME: u64 = 1742213400;

        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or(BlobLoaderError::TransactionNotFound(tx_hash))?;

        let blob_versioned_hashes = tx
            .blob_versioned_hashes()
            .ok_or(BlobLoaderError::BlobVersionedHashesNotFound(tx_hash))?;
        let blob_versioned_hashes = HashSet::<_, RandomState>::from_iter(blob_versioned_hashes);
        let block_hash = tx
            .block_hash
            .ok_or(BlobLoaderError::TransactionBlockNotFound(tx_hash))?;
        let block = self
            .provider
            .get_block_by_hash(block_hash)
            .await?
            .ok_or(BlobLoaderError::BlockNotFound(H256(block_hash.0)))?;
        let slot = (block.header.timestamp - BEACON_GENESIS_BLOCK_TIME)
            / self.config.beacon_block_time.as_secs();

        let attempts = attempts.unwrap_or(0);
        let mut count = 0;
        let blob_bundle = loop {
            log::trace!("trying to get blob, attempt #{}", count + 1);
            let blob_bundle_result = self.read_blob_bundle(slot).await;
            if blob_bundle_result.is_ok() || count >= attempts {
                break blob_bundle_result;
            } else {
                time::sleep(self.config.beacon_block_time).await;
                count += 1;
            }
        }?;

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
            .ok_or(BlobLoaderError::DecodeBlobs)?
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
    futures: FuturesUnordered<BoxFuture<'static, Result<CodeAndIdUnchecked>>>,
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
        let res = futures::ready!(self.futures.poll_next_unpin(cx)).map(|result| {
            let code_and_id = result?;
            self.codes_loading.remove(&code_and_id.code_id);
            Ok(BlobLoaderEvent::BlobLoaded(code_and_id))
        });

        res.map_or(Poll::Pending, |res| Poll::Ready(Some(res)))
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

            let CodeBlobInfo { tx_hash, .. } = self
                .db
                .code_blob_info(code_id)
                .ok_or(BlobLoaderError::CodeBlobInfoNotFound(code_id))?;

            if let Some(code) = self.db.original_code(code_id) {
                log::warn!("Code {code_id} is already loaded, skipping loading from remote source");
                self.futures
                    .push(futures::future::ready(Ok(CodeAndIdUnchecked { code_id, code })).boxed());
                continue;
            }

            if let Some(code) = self.db.original_code(code_id) {
                log::warn!("Code {code_id} is already loaded, skipping loading from remote source");
                self.futures
                    .push(futures::future::ready(Ok(CodeAndIdUnchecked { code_id, code })).boxed());
                continue;
            }

            self.codes_loading.insert(code_id);
            self.futures.push(
                self.blobs_reader
                    .clone()
                    .read_code_from_tx_hash(code_id, tx_hash, attempts)
                    .boxed(),
            );
        }

        Ok(())
    }
}
