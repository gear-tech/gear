// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// TODO #4552: add tests for observer utils

use alloy::{
    contract::Event,
    network::{Ethereum, Network},
    providers::{Provider as _, RootProvider},
    rpc::{
        client::BatchRequest,
        types::{
            Block, Log,
            eth::{Filter, Topic},
        },
    },
};
use anyhow::{Context, Result};
use ethexe_common::{Address, BlockData, BlockHeader, SimpleBlockData, events::BlockEvent};
use ethexe_ethereum::{abi::IRouter, mirror, router};
use futures::{TryFutureExt, future};
use gprimitives::H256;
use std::{collections::HashMap, future::IntoFuture, ops::RangeInclusive};

// TODO: #4562 append also a configurable batch size parameter
/// Max number of blocks per `eth_getBlockByNumber` JSON-RPC batch.
const MAX_BLOCK_BATCH_SIZE: usize = 256;
/// Block-window size passed to alloy's [`alloy::contract::ChunkedEvent`] when fetching logs.
const LOGS_CHUNK_SIZE: u64 = 256;
/// Maximum number of in-flight log chunk requests issued by [`alloy::contract::ChunkedEvent`].
const LOGS_MAX_CONCURRENCY: usize = 8;

#[derive(Debug, Copy, Clone, PartialEq, Eq, derive_more::From)]
pub enum BlockId {
    Hash(H256),
    Latest,
    Finalized,
}

impl BlockId {
    fn as_alloy(self) -> alloy::eips::BlockId {
        match self {
            BlockId::Hash(hash) => alloy::eips::BlockId::hash(hash.0.into()),
            BlockId::Latest => alloy::eips::BlockId::latest(),
            BlockId::Finalized => alloy::eips::BlockId::finalized(),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait BlockLoader {
    async fn load_simple(&self, block: BlockId) -> Result<SimpleBlockData>;

    async fn load(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData>;

    async fn load_many(&self, range: RangeInclusive<u64>) -> Result<HashMap<H256, BlockData>>;
}

#[derive(Debug, Clone)]
pub struct EthereumBlockLoader {
    provider: RootProvider,
    router_address: Address,
    logs_chunk_size: u64,
    logs_max_concurrency: usize,
}

impl EthereumBlockLoader {
    pub fn new(provider: RootProvider, router_address: Address) -> Self {
        Self {
            provider,
            router_address,
            logs_chunk_size: LOGS_CHUNK_SIZE,
            logs_max_concurrency: LOGS_MAX_CONCURRENCY,
        }
    }

    pub fn with_logs_chunk_size(mut self, chunk_size: u64) -> Self {
        self.logs_chunk_size = chunk_size;
        self
    }

    pub fn with_logs_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.logs_max_concurrency = max_concurrency;
        self
    }

    fn log_filter() -> Filter {
        let topic = Topic::from_iter(
            [
                router::events::signatures::ALL,
                mirror::events::signatures::ALL,
            ]
            .into_iter()
            .flatten()
            .copied(),
        );

        Filter::new().event_signature(topic)
    }

    fn logs_to_events(&self, logs: Vec<Log>) -> Result<HashMap<H256, Vec<BlockEvent>>> {
        let block_hash_of = |log: &Log| -> Result<H256> {
            log.block_hash
                .map(|v| v.0.into())
                .context("block hash is missing")
        };

        let mut res: HashMap<_, Vec<_>> = HashMap::new();

        for log in logs {
            let block_hash = block_hash_of(&log)?;
            let address = log.address();

            if address.0 == self.router_address.0 {
                if let Some(event) = router::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            } else {
                let address = (*address.into_word()).into();

                if let Some(event) = mirror::events::try_extract_event(&log)? {
                    res.entry(block_hash)
                        .or_default()
                        .push(BlockEvent::mirror(address, event));
                }
            }
        }

        Ok(res)
    }

    fn block_response_to_data(block: Block) -> (H256, BlockHeader) {
        let block_hash = H256(block.header.hash.0);

        let header = BlockHeader {
            height: block.header.number as u32,
            timestamp: block.header.timestamp,
            parent_hash: H256(block.header.parent_hash.0),
        };

        (block_hash, header)
    }

    /// Fetches block headers for `range` via a single `eth_getBlockByNumber` JSON-RPC batch.
    ///
    /// The caller is responsible for keeping batches within the provider's allowed batch size,
    /// see [`MAX_BLOCK_BATCH_SIZE`].
    async fn request_block_headers(&self, range: RangeInclusive<u64>) -> Result<Vec<Block>> {
        let mut batch = BatchRequest::new(self.provider.client());
        let headers_request = range
            .map(|bn| {
                batch
                    .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                        "eth_getBlockByNumber",
                        &(format!("0x{bn:x}"), false),
                    )
                    .expect("infallible")
            })
            .collect::<Vec<_>>();
        batch.send().await?;

        let mut blocks = Vec::new();
        for response in future::join_all(headers_request).await {
            let Some(block) = response? else {
                break;
            };
            blocks.push(block);
        }
        Ok(blocks)
    }

    /// Fetches all router/mirror logs for `range` using alloy's chunked-event helper.
    ///
    /// The helper attempts the full range first, then splits into `logs_chunk_size`-block
    /// windows (default [`LOGS_CHUNK_SIZE`]) queried up to `logs_max_concurrency`
    /// (default [`LOGS_MAX_CONCURRENCY`]) at a time, and finally falls back to
    /// per-block queries for any chunk that still fails.
    async fn request_logs(&self, range: RangeInclusive<u64>) -> Result<Vec<Log>> {
        let filter = Self::log_filter()
            .from_block(*range.start())
            .to_block(*range.end());

        // The event type parameter is unused by `query_raw`, which returns undecoded logs;
        // we pass `IRouter::BatchCommitted` solely to satisfy the `SolEvent` trait bound.
        let chunked = Event::<_, IRouter::BatchCommitted>::new(self.provider.clone(), filter)
            .chunked()
            .chunk_size(self.logs_chunk_size)
            .concurrent(self.logs_max_concurrency);

        chunked
            .query_raw()
            .await
            .context("failed to fetch logs via alloy ChunkedEvent")
    }
}

impl BlockLoader for EthereumBlockLoader {
    async fn load_simple(&self, block: BlockId) -> Result<SimpleBlockData> {
        log::trace!("Querying simple data for one block {block:?}");
        let block = self
            .provider
            .get_block(block.as_alloy())
            .into_future()
            .await?;
        let block = block.context("block not found")?;
        let (hash, header) = Self::block_response_to_data(block);
        Ok(SimpleBlockData { hash, header })
    }

    async fn load(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData> {
        let filter = Self::log_filter().at_block_hash(block.0);
        // Preserve concrete error type so SyncError's classifier can downcast.
        let logs_request = self.provider.get_logs(&filter).map_err(anyhow::Error::from);

        let (block_hash, header, logs) = if let Some(header) = header {
            (block, header, logs_request.await?)
        } else {
            let data = self.load_simple(block.into());
            let (SimpleBlockData { hash, header }, logs) =
                future::try_join(data, logs_request).await?;
            (hash, header, logs)
        };
        anyhow::ensure!(
            block_hash == block,
            "expected block hash {block}, got {block_hash}"
        );

        let events = self.logs_to_events(logs)?;
        anyhow::ensure!(
            events.len() <= 1,
            "expected events for at most 1 block, but got for {}",
            events.len()
        );

        let (block_hash, events) = events
            .into_iter()
            .next()
            .unwrap_or_else(|| (block_hash, Vec::new()));
        anyhow::ensure!(
            block_hash == block,
            "expected block hash {block}, got {block_hash}"
        );

        Ok(BlockData {
            hash: block,
            header,
            events,
        })
    }

    async fn load_many(&self, range: RangeInclusive<u64>) -> Result<HashMap<H256, BlockData>> {
        if range.is_empty() {
            return Ok(HashMap::new());
        }
        log::trace!("Querying blocks batch in {range:?} range");

        let header_batches = range.clone().step_by(MAX_BLOCK_BATCH_SIZE).map(|start| {
            let end = (start + MAX_BLOCK_BATCH_SIZE as u64 - 1).min(*range.end());
            self.request_block_headers(start..=end)
        });

        let (headers_batches, logs) = future::try_join(
            future::try_join_all(header_batches),
            self.request_logs(range),
        )
        .await?;

        let mut events = self.logs_to_events(logs)?;
        let mut blocks_data: HashMap<H256, BlockData> = HashMap::new();
        for block in headers_batches.into_iter().flatten() {
            let (hash, header) = Self::block_response_to_data(block);
            let events = events.remove(&hash).unwrap_or_default();
            blocks_data.insert(
                hash,
                BlockData {
                    hash,
                    header,
                    events,
                },
            );
        }

        Ok(blocks_data)
    }
}
