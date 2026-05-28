// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloy::{
    consensus::{SidecarCoder, SimpleCoder, Transaction},
    primitives::B256,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::beacon::{genesis::GenesisResponse, sidecar::GetBlobsResponse},
    transports::{RpcError, TransportErrorKind},
};
use ethexe_common::{
    CodeAndIdUnchecked, CodeBlobInfo,
    db::{CodesStorageRO, OnChainStorageRO},
    gear_core::ids::prelude::CodeIdExt,
};
use futures::{
    FutureExt, Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
};
use gprimitives::{CodeId, H256};
use reqwest::Client;
use std::{collections::HashSet, fmt, num::NonZero, pin::Pin, task::Poll};
use tokio::{sync::OnceCell, time::Duration};

#[derive(Clone, PartialEq, Eq)]
pub enum BlobLoaderEvent {
    BlobLoaded(CodeAndIdUnchecked),
}

#[derive(thiserror::Error, Debug)]
pub enum BlobLoaderError {
    #[error("transport error: {0}")]
    Transport(#[from] RpcError<TransportErrorKind>),
    #[error("failed to get code blob info for: {0}")]
    CodeBlobInfoNotFound(CodeId),
}

#[derive(thiserror::Error, Debug)]
enum ReadBlobBundleError {
    #[error("failed to read blob bundle from beacon node: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("failed to decode blob bundle response: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(thiserror::Error, Debug)]
enum ReaderError {
    #[error("transport error: {0}")]
    Transport(#[from] RpcError<TransportErrorKind>),
    #[error("failed to find transaction by hash: {0}")]
    TransactionNotFound(H256),
    #[error("failed to get blob versioned hashes from transaction: {0}")]
    BlobVersionedHashesNotFound(H256),
    #[error("failed to get transaction block hash: {0}")]
    TransactionBlockNotFound(H256),
    #[error("failed to get block by hash: {0}")]
    BlockNotFound(H256),
    #[error("failed to read blob bundle: {0}")]
    ReadBlob(#[from] ReadBlobBundleError),
    #[error("failed to decode blobs")]
    DecodeBlobs,
    #[error("failed to access genesis time: {0}")]
    GenesisTimeAccess(reqwest::Error),
    #[error("received empty blob")]
    EmptyBlob,
    #[error("blob code id mismatch: expected {expected:?}, found {found:?}")]
    CodeIdMismatch { expected: CodeId, found: CodeId },
}

type LoaderResult<T> = Result<T, BlobLoaderError>;
type ReaderResult<T> = Result<T, ReaderError>;

pub trait BlobLoaderService:
    Stream<Item = LoaderResult<BlobLoaderEvent>> + FusedStream + Send + Unpin
{
    fn load_codes(&mut self, codes: HashSet<CodeId>) -> LoaderResult<()>;

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
    pub attempts: NonZero<u8>,
}

#[derive(Clone)]
struct ConsensusLayerBlobReader {
    provider: RootProvider,
    http_client: Client,
    config: ConsensusLayerConfig,
}

impl ConsensusLayerBlobReader {
    async fn read_blob(&self, expected_code_id: CodeId, tx_hash: H256) -> ReaderResult<Vec<u8>> {
        let mut last_err = None;
        let mut previously_received_code_id = None;
        for attempt in 0..self.config.attempts.get() {
            log::trace!("trying to get blob, attempt #{attempt}");
            match self.try_query_blob(tx_hash).await {
                Ok(blob) => {
                    match handle_blob(
                        blob,
                        expected_code_id,
                        &mut previously_received_code_id,
                        attempt,
                    ) {
                        Ok(blob) => return Ok(blob),
                        Err(err) => last_err = Some(err),
                    }
                }
                Err(err) => {
                    log::warn!("failed to get blob on attempt #{attempt}: {err}");
                    last_err = Some(err);
                }
            }

            tokio::time::sleep(self.config.beacon_block_time).await;
        }

        Err(last_err.expect("Must be set, because attempts is not zero"))
    }

    async fn try_query_blob(&self, tx_hash: H256) -> ReaderResult<Vec<u8>> {
        use ReaderError::*;

        let tx = self
            .provider
            .get_transaction_by_hash(tx_hash.0.into())
            .await?
            .ok_or(TransactionNotFound(tx_hash))?;

        // TODO: #5102 here may be a problem with code if it has same versioned hashes,
        // consider to change it to more reliable way.
        let blob_versioned_hashes = tx
            .blob_versioned_hashes()
            .ok_or(BlobVersionedHashesNotFound(tx_hash))?;

        let block_hash = tx.block_hash.ok_or(TransactionBlockNotFound(tx_hash))?;

        let block = self
            .provider
            .get_block_by_hash(block_hash)
            .await?
            .ok_or(BlockNotFound(H256(block_hash.0)))?;

        // detect anvil by chain id
        let slot = if let Some(chain_id) = tx.chain_id()
            && chain_id == 31337
        {
            block.header.number
        } else {
            static BEACON_GENESIS_BLOCK_TIME: OnceCell<u64> = OnceCell::const_new();

            let beacon_genesis_block_time = *BEACON_GENESIS_BLOCK_TIME
                .get_or_try_init(|| self.read_genesis_time())
                .await
                .map_err(GenesisTimeAccess)?;
            (block.header.timestamp - beacon_genesis_block_time)
                / self.config.beacon_block_time.as_secs()
        };

        let blob_bundle = self
            .read_blob_bundle(slot, blob_versioned_hashes)
            .await
            .map_err(ReadBlob)?;

        let mut coder = SimpleCoder::default();
        let data = coder
            .decode_all(&blob_bundle.data)
            .ok_or(DecodeBlobs)?
            .concat();

        Ok(data)
    }

    async fn read_genesis_time(&self) -> reqwest::Result<u64> {
        let ethereum_beacon_rpc = &self.config.ethereum_beacon_rpc;
        let response = self
            .http_client
            .get(format!("{ethereum_beacon_rpc}/eth/v1/beacon/genesis"))
            .send()
            .await?
            .json::<GenesisResponse>()
            .await?;

        Ok(response.data.genesis_time)
    }

    async fn read_blob_bundle(
        &self,
        slot: u64,
        versioned_hashes: &[B256],
    ) -> Result<GetBlobsResponse, ReadBlobBundleError> {
        let ethereum_beacon_rpc = &self.config.ethereum_beacon_rpc;
        let bytes = self
            .http_client
            .get(format!(
                "{ethereum_beacon_rpc}/eth/v1/beacon/blobs/{slot}?versioned_hashes={}",
                versioned_hashes
                    .iter()
                    .map(|versioned_hash| versioned_hash.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ))
            .send()
            .await?
            .bytes()
            .await?;
        let blobs_response =
            serde_json::from_slice::<GetBlobsResponse>(&bytes).inspect_err(|err| {
                log::trace!("failed to decode blob bundle response: {err}, bytes: {bytes:?}")
            })?;
        Ok(blobs_response)
    }
}

pub trait Database: CodesStorageRO + OnChainStorageRO + Unpin + Send + Clone + 'static {}
impl<T: CodesStorageRO + OnChainStorageRO + Unpin + Send + Clone + 'static> Database for T {}

pub struct BlobLoader<DB: Database> {
    futures: FuturesUnordered<BoxFuture<'static, ReaderResult<CodeAndIdUnchecked>>>,
    codes_loading: HashSet<CodeId>,

    blobs_reader: ConsensusLayerBlobReader,
    db: DB,
}

impl<DB: Database> Stream for BlobLoader<DB> {
    type Item = LoaderResult<BlobLoaderEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match futures::ready!(self.futures.poll_next_unpin(cx)) {
            None => Poll::Pending,
            Some(Err(err)) => {
                // TODO: #4995 currently in case of error in blob loading we just skip it
                log::error!("Failed to load blob: {err}, skipping");
                Poll::Pending
            }
            Some(Ok(code_and_id)) => {
                self.codes_loading.remove(&code_and_id.code_id);
                Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(code_and_id))))
            }
        }
    }
}

impl<DB: Database> FusedStream for BlobLoader<DB> {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<DB: Database> BlobLoader<DB> {
    pub async fn new(db: DB, consensus_cfg: ConsensusLayerConfig) -> LoaderResult<Self> {
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

    #[cfg(test)]
    fn new_with_consensus_reader(db: DB, blobs_reader: ConsensusLayerBlobReader) -> Self {
        Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),
            blobs_reader,
            db,
        }
    }
}

impl<DB: Database> BlobLoaderService for BlobLoader<DB> {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.codes_loading.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>) -> LoaderResult<()> {
        for code_id in codes {
            if self.codes_loading.contains(&code_id) {
                continue;
            }

            let CodeBlobInfo { tx_hash, .. } = self
                .db
                .code_blob_info(code_id)
                .ok_or(BlobLoaderError::CodeBlobInfoNotFound(code_id))?;

            self.codes_loading.insert(code_id);

            if let Some(code) = self.db.original_code(code_id) {
                log::warn!("Code {code_id} is already loaded, skipping loading from remote source");
                self.futures
                    .push(futures::future::ready(Ok(CodeAndIdUnchecked { code_id, code })).boxed());
            } else {
                let blobs_reader = self.blobs_reader.clone();
                self.futures.push(
                    async move {
                        blobs_reader
                            .read_blob(code_id, tx_hash)
                            .await
                            .map(|code| CodeAndIdUnchecked { code_id, code })
                    }
                    .boxed(),
                );
            }
        }

        Ok(())
    }
}

// TODO: #4995 temporary solution to protect against inconsistent blob data,
// we have second check of code id in ethexe-processor in handle_new_code as well,
// so this solution must be reconsidered.
fn handle_blob(
    blob: Vec<u8>,
    code_id: CodeId,
    previously_received_code_id: &mut Option<CodeId>,
    attempt: u8,
) -> ReaderResult<Vec<u8>> {
    if blob.is_empty() {
        log::warn!("received empty blob on attempt #{attempt}");
        return Err(ReaderError::EmptyBlob);
    }

    let received_code_id = CodeId::generate(&blob);
    if *previously_received_code_id == Some(received_code_id) {
        log::warn!(
            "received same blob with invalid id on attempt #{attempt}, code id: {received_code_id:?}, consider blob as loaded then",
        );
        return Ok(blob);
    }

    if code_id != received_code_id {
        *previously_received_code_id = Some(received_code_id);
        log::warn!(
            "received blob code id mismatch on attempt #{attempt}: expected {code_id:?}, got {received_code_id:?}",
        );
        return Err(ReaderError::CodeIdMismatch {
            expected: code_id,
            found: received_code_id,
        });
    }

    Ok(blob)
}

#[cfg(test)]
mod tests;
