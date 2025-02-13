// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Ethereum state observer for ethexe.

use alloy::{
    network::{Ethereum, Network},
    primitives::Address as AlloyAddress,
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::{
        client::BatchRequest,
        types::{eth::Header, Block, Filter, Log, Topic},
    },
    transports::BoxTransport,
};
use anyhow::{anyhow, Context as _, Result};
use ethexe_common::{
    db::BlocksOnChainData,
    events::{BlockEvent, RouterEvent},
    BlockData, SimpleBlockData,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodeInfo};
use ethexe_ethereum::{
    mirror,
    router::{self, RouterQuery},
    wvara,
};
use ethexe_signer::Address;
use futures::{
    future::{self, BoxFuture},
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

// +_+_+ change codes to futures map
// use futures_bounded::FuturesMap;

pub type Provider = RootProvider<BoxTransport>;

mod blobs;
mod observer;
mod query;

#[cfg(test)]
mod tests;

pub use blobs::*;
pub use observer::*;
pub use query::*;

type BlobDownloadFuture = BoxFuture<'static, Result<(CodeId, u64, Vec<u8>)>>;
type BlockFuture = BoxFuture<'static, Result<(H256, Vec<(CodeId, CodeInfo)>)>>;

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObserverEvent {
    Blob {
        code_id: CodeId,
        timestamp: u64,
        code: Vec<u8>,
    },
    Block(SimpleBlockData),
    BlockSynced(H256),
}

pub struct ObserverService {
    provider: Provider,
    database: Box<dyn BlocksOnChainData>,
    blobs_reader: Arc<dyn BlobReader>,
    subscription: Subscription<Header>,

    router: Address,
    wvara_address: Address,

    last_block_number: u32,

    headers_stream: SubscriptionStream<Header>,
    block_sync_queue: VecDeque<Header>,
    sync_future: Option<BlockFuture>,

    codes_futures: FuturesUnordered<BlobDownloadFuture>,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(res) = self.headers_stream.poll_next_unpin(cx) {
            let Some(header) = res else {
                // TODO (breathx): test resubscribe works.
                log::warn!("Alloy headers stream ended, resubscribing");
                self.headers_stream = self.subscription.resubscribe().into_stream();
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };

            let data = SimpleBlockData {
                hash: H256(header.hash.0),
                header: BlockHeader {
                    height: header.number as u32,
                    timestamp: header.timestamp,
                    parent_hash: H256(header.parent_hash.0),
                },
            };

            log::trace!("Received a new block: {data:?}");

            self.block_sync_queue.push_back(header);

            return Poll::Ready(Some(Ok(ObserverEvent::Block(data))));
        }

        if self.sync_future.is_none() {
            if let Some(header) = self.block_sync_queue.pop_front() {
                let sync = ChainSync {
                    provider: self.provider.clone(),
                    database: self.database.clone_boxed(),
                    blobs_reader: self.blobs_reader.clone(),
                    router_address: self.router.0.into(),
                    wvara_address: self.wvara_address.0.into(),
                    max_sync_depth: 10_000,
                    heuristic_sync_depth: 2,
                };
                self.sync_future = Some(Box::pin(sync.sync(header)));
            }
        }

        if let Some(fut) = self.sync_future.as_mut() {
            if let Poll::Ready(res) = fut.poll_unpin(cx) {
                self.sync_future = None;

                let res = res.map(|(hash, codes)| {
                    for (code_id, code_info) in codes {
                        self.lookup_code(code_id, code_info.timestamp, code_info.tx_hash);
                    }

                    ObserverEvent::BlockSynced(hash)
                });

                return Poll::Ready(Some(res));
            }
        }

        if let Poll::Ready(Some(res)) = self.codes_futures.poll_next_unpin(cx) {
            let event = res.map(|(code_id, timestamp, code)| ObserverEvent::Blob {
                code_id,
                timestamp,
                code,
            });

            return Poll::Ready(Some(event));
        }

        Poll::Pending
    }
}

impl FusedStream for ObserverService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl ObserverService {
    pub async fn new<DB>(
        eth_cfg: &EthereumConfig,
        db: &DB,
        blobs_reader: Option<Arc<dyn BlobReader>>,
    ) -> Result<Self>
    where
        DB: BlocksOnChainData + BlockMetaStorage,
    {
        let EthereumConfig {
            rpc,
            beacon_rpc,
            router_address,
            block_time,
        } = eth_cfg;

        let blobs_reader = match blobs_reader {
            Some(reader) => reader,
            None => Arc::new(
                ConsensusLayerBlobReader::new(rpc, beacon_rpc, *block_time)
                    .await
                    .context("failed to create blob reader")?,
            ),
        };

        let router_query = RouterQuery::new(rpc, *router_address).await?;

        let wvara_address = Address(router_query.wvara_address().await?.0 .0);

        let provider = ProviderBuilder::new()
            .on_builtin(rpc)
            .await
            .context("failed to create ethereum provider")?;

        let subscription = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?;

        Self::pre_process_genesis_for_db(db, &provider, &router_query).await?;

        let headers_stream = subscription.resubscribe().into_stream();

        Ok(Self {
            provider,
            database: BlocksOnChainData::clone_boxed(db),
            blobs_reader,
            subscription,
            router: *router_address,
            wvara_address,
            last_block_number: 0,
            headers_stream,
            codes_futures: Default::default(),
            block_sync_queue: Default::default(),
            sync_future: Default::default(),
        })
    }

    pub fn provider(&self) -> &Provider {
        &self.provider
    }

    pub fn status(&self) -> ObserverStatus {
        ObserverStatus {
            eth_best_height: self.last_block_number,
            pending_codes: self.codes_futures.len(),
        }
    }

    pub async fn query_block_by_hash(&self, hash: H256) -> Result<SimpleBlockData> {
        let block = self
            .provider
            .get_block_by_hash(hash.0.into(), Default::default())
            .await?
            .ok_or_else(|| anyhow!("Genesis block with hash {hash:?} not found by rpc"))?;

        Ok(SimpleBlockData {
            hash,
            header: BlockHeader {
                height: block.header.number as u32,
                timestamp: block.header.timestamp,
                parent_hash: H256(block.header.parent_hash.0),
            },
        })
    }

    fn lookup_code(&mut self, code_id: CodeId, timestamp: u64, tx_hash: H256) {
        self.codes_futures.push(Box::pin(read_code_from_tx_hash(
            self.blobs_reader.clone(),
            code_id,
            timestamp,
            tx_hash,
            Some(3),
        )));
    }

    // TODO (gsobol): this is a temporary solution consider where to move it in better place, out of ObserverService.
    /// If genesis block is not yet fully setup in the database, we need to do it
    async fn pre_process_genesis_for_db<DB>(
        db: &DB,
        provider: &Provider,
        router_query: &RouterQuery,
    ) -> Result<()>
    where
        DB: BlocksOnChainData + BlockMetaStorage,
    {
        let genesis_block_hash = router_query.genesis_block_hash().await?;

        if BlockMetaStorage::block_end_state_is_valid(db, genesis_block_hash).unwrap_or(false) {
            return Ok(());
        }

        let genesis_block = provider
            .get_block_by_hash(genesis_block_hash.0.into(), Default::default())
            .await?
            .ok_or_else(|| {
                anyhow!("Genesis block with hash {genesis_block_hash:?} not found by rpc")
            })?;

        let genesis_header = BlockHeader {
            height: genesis_block.header.number as u32,
            timestamp: genesis_block.header.timestamp,
            parent_hash: H256(genesis_block.header.parent_hash.0),
        };

        BlocksOnChainData::set_block_header(db, genesis_block_hash, &genesis_header);
        BlocksOnChainData::set_block_events(db, genesis_block_hash, &[]);

        db.set_latest_synced_block_height(genesis_header.height);
        db.set_block_is_synced(genesis_block_hash);

        db.set_block_commitment_queue(genesis_block_hash, Default::default());
        db.set_previous_committed_block(genesis_block_hash, H256::zero());
        db.set_block_is_empty(genesis_block_hash, true);
        db.set_block_end_program_states(genesis_block_hash, Default::default());
        db.set_block_end_schedule(genesis_block_hash, Default::default());

        db.set_latest_valid_block(genesis_block_hash, genesis_header);

        db.set_block_end_state_is_valid(genesis_block_hash, true);

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserverStatus {
    pub eth_best_height: u32,
    pub pending_codes: usize,
}

fn router_and_wvara_filter(
    filter: Filter,
    wvara_address: AlloyAddress,
    router_address: AlloyAddress,
) -> Filter {
    let router_and_wvara_topic = Topic::from_iter(
        router::events::signatures::ALL
            .iter()
            .chain(wvara::events::signatures::ALL)
            .cloned(),
    );

    filter
        .clone()
        .address(vec![router_address, wvara_address])
        .event_signature(router_and_wvara_topic)
}

fn mirrors_filter(filter: Filter) -> Filter {
    filter.event_signature(Topic::from_iter(
        mirror::events::signatures::ALL.iter().cloned(),
    ))
}

fn logs_to_events(
    router_and_wvara_logs: Vec<Log>,
    mirrors_logs: Vec<Log>,
    router_address: AlloyAddress,
    wvara_address: AlloyAddress,
) -> Result<HashMap<H256, Vec<BlockEvent>>> {
    let block_hash_of = |log: &Log| -> Result<H256> {
        log.block_hash
            .map(|v| v.0.into())
            .ok_or(anyhow!("Block hash is missing"))
    };

    let mut res: HashMap<_, Vec<_>> = HashMap::new();

    for log in router_and_wvara_logs {
        let block_hash = block_hash_of(&log)?;

        match log.address() {
            address if address == router_address => {
                if let Some(event) = router::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            }
            address if address == wvara_address => {
                if let Some(event) = wvara::events::try_extract_event(&log)? {
                    res.entry(block_hash).or_default().push(event.into());
                }
            }
            _ => unreachable!("Unexpected address in log"),
        }
    }

    for mirror_log in mirrors_logs {
        let block_hash = block_hash_of(&mirror_log)?;

        let address = (*mirror_log.address().into_word()).into();

        if let Some(event) = mirror::events::try_extract_event(&mirror_log)? {
            res.entry(block_hash)
                .or_default()
                .push(BlockEvent::mirror(address, event));
        }
    }

    Ok(res)
}

pub fn block_response_to_data(response: Option<Block>) -> Result<(H256, BlockHeader)> {
    let block = response.ok_or_else(|| anyhow!("Block not found"))?;
    let block_hash = H256(block.header.hash.0);

    let header = BlockHeader {
        height: block.header.number as u32,
        timestamp: block.header.timestamp,
        parent_hash: H256(block.header.parent_hash.0),
    };

    Ok((block_hash, header))
}

struct ChainSync {
    pub provider: Provider,
    pub database: Box<dyn BlocksOnChainData>,
    pub blobs_reader: Arc<dyn BlobReader>,
    pub router_address: AlloyAddress,
    pub wvara_address: AlloyAddress,
    pub max_sync_depth: u32,
    pub heuristic_sync_depth: u32,
}

impl ChainSync {
    async fn load_blocks_batch_data(
        provider: Provider,
        router_address: AlloyAddress,
        wvara_address: AlloyAddress,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<BlockData>> {
        log::trace!("Querying blocks from {from_block} to {to_block}");

        let mut batch = BatchRequest::new(provider.client());

        let headers_request: FuturesUnordered<_> = (from_block..=to_block)
            .map(|bn| {
                batch
                    .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                        "eth_getBlockByNumber",
                        &(format!("0x{bn:x}"), false),
                    )
                    .expect("infallible")
                    .boxed()
            })
            .collect();

        batch.send().await?;

        let filter = Filter::new().from_block(from_block).to_block(to_block);

        let mirrors_filter = mirrors_filter(filter.clone());
        let router_and_wvara_filter =
            router_and_wvara_filter(filter, router_address, wvara_address);

        let logs_request = future::try_join(
            provider.get_logs(&router_and_wvara_filter),
            provider.get_logs(&mirrors_filter),
        );

        let (blocks, logs) = future::join(future::join_all(headers_request), logs_request).await;
        let (router_and_wvara_logs, mirrors_logs) = logs?;

        let mut blocks_data = Vec::new();

        for response in blocks {
            let block = response?.ok_or_else(|| anyhow!("Block not found"))?;
            let block_hash = H256(block.header.hash.0);

            let header = BlockHeader {
                height: block.header.number as u32,
                timestamp: block.header.timestamp,
                parent_hash: H256(block.header.parent_hash.0),
            };

            blocks_data.push(BlockData {
                hash: block_hash,
                header,
                events: Vec::new(),
            });
        }

        let mut events = logs_to_events(
            router_and_wvara_logs,
            mirrors_logs,
            router_address,
            wvara_address,
        )?;
        for block_data in blocks_data.iter_mut() {
            block_data.events = events.remove(&block_data.hash).unwrap_or_default();
        }

        Ok(blocks_data)
    }

    async fn load_block_data(&self, block: H256, header: Option<BlockHeader>) -> Result<BlockData> {
        log::trace!("Querying data for one block {block:?}");

        let filter = Filter::new().at_block_hash(block.0);
        let mirrors_filter = mirrors_filter(filter.clone());
        let router_and_wvara_filter =
            router_and_wvara_filter(filter, self.router_address, self.wvara_address);

        let logs_request = future::try_join(
            self.provider.get_logs(&router_and_wvara_filter),
            self.provider.get_logs(&mirrors_filter),
        );

        let ((block_hash, header), (router_and_wvara_logs, mirrors_logs)) =
            if let Some(header) = header {
                ((block, header), logs_request.await?)
            } else {
                let block_request = self.provider.get_block_by_hash(
                    block.0.into(),
                    alloy::rpc::types::BlockTransactionsKind::Hashes,
                );

                match future::try_join(block_request, logs_request).await {
                    Ok((response, logs)) => (block_response_to_data(response)?, logs),
                    Err(err) => Err(err)?,
                }
            };

        if block_hash != block {
            return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
        }

        let events = logs_to_events(
            router_and_wvara_logs,
            mirrors_logs,
            self.router_address,
            self.wvara_address,
        )?;

        if events.len() > 1 {
            return Err(anyhow!(
                "Expected events for at most 1 block, but got for {}",
                events.len()
            ));
        }

        let (block_hash, events) = events
            .into_iter()
            .next()
            .unwrap_or_else(|| (block_hash, Vec::new()));

        if block_hash != block {
            return Err(anyhow!("Expected block hash {block}, got {block_hash}"));
        }

        Ok(BlockData {
            hash: block,
            header,
            events,
        })
    }

    async fn load_blocks_data(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<HashMap<H256, BlockData>> {
        let batch_futures: FuturesUnordered<_> = (from_block..=to_block)
            .step_by(MAX_QUERY_BLOCK_RANGE)
            .map(|start| {
                let end = (start + MAX_QUERY_BLOCK_RANGE as u64 - 1).min(to_block);

                Self::load_blocks_batch_data(
                    self.provider.clone(),
                    self.router_address,
                    self.wvara_address,
                    start,
                    end,
                )
                .boxed()
            })
            .collect();

        future::try_join_all(batch_futures).await.map(|batches| {
            batches
                .into_iter()
                .flat_map(|batch| {
                    batch
                        .into_iter()
                        .map(|block_data| (block_data.hash, block_data))
                })
                .collect()
        })
    }

    pub async fn sync(self, chain_head: Header) -> Result<(H256, Vec<(CodeId, CodeInfo)>)> {
        let latest_synced_block_height =
            self.database
                .latest_synced_block_height()
                .unwrap_or_else(|| {
                    unreachable!("latest_synced_block_height must be set in ObserverService::new")
                });

        let header = BlockHeader {
            height: chain_head.number as u32,
            timestamp: chain_head.timestamp,
            parent_hash: H256(chain_head.parent_hash.0),
        };

        let mut blocks_data = if header.height <= latest_synced_block_height {
            log::warn!(
                "Get a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest_synced_block_height
            );
            Default::default()
        } else {
            if (header.height - latest_synced_block_height) >= self.max_sync_depth {
                // TODO (gsobol): return an event to notify about too deep chain.
                return Err(anyhow!(
                    "Too much to sync: current block number: {}, Latest valid block number: {}, Max depth: {}",
                    header.height,
                    latest_synced_block_height,
                    self.max_sync_depth
                ));
            }

            if header.height - latest_synced_block_height > self.heuristic_sync_depth {
                self.load_blocks_data(latest_synced_block_height as u64, header.height as u64)
                    .await?
            } else {
                Default::default()
            }
        };

        let mut codes_to_load_now = HashSet::new();
        let mut codes_to_load_later = HashMap::new();
        let mut chain = Vec::new();

        let mut hash = H256(chain_head.hash.0);
        while !self.database.block_is_synced(hash) {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    self.load_block_data(
                        hash,
                        (hash == H256(chain_head.hash.0)).then_some(header.clone()),
                    )
                    .await?
                }
            };

            if hash != block_data.hash {
                unreachable!(
                    "Expected data for block hash {hash}, got for {}",
                    block_data.hash
                );
            }

            for event in &block_data.events {
                match event {
                    BlockEvent::Router(RouterEvent::CodeValidationRequested {
                        code_id,
                        timestamp,
                        tx_hash,
                    }) => {
                        let code_info = CodeInfo {
                            timestamp: *timestamp,
                            tx_hash: *tx_hash,
                        };
                        self.database.set_code_info(*code_id, code_info.clone());

                        if !self.database.original_code_exists(*code_id) {
                            codes_to_load_later.insert(*code_id, code_info);
                        }
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid }) => {
                        if *valid && !self.database.original_code_exists(*code_id) {
                            let _ = codes_to_load_later.remove(code_id);
                            codes_to_load_now.insert(*code_id);
                        }
                    }
                    _ => {}
                }
            }

            self.database.set_block_header(hash, &block_data.header);
            self.database.set_block_events(hash, &block_data.events);

            chain.push(hash);

            hash = block_data.header.parent_hash;
        }

        // TODO (gsobol): this is a temporary solution to load already validated codes.
        // Must be done with ObserverService::codes_futures together.
        let codes_futures = FuturesUnordered::new();
        for code_id in codes_to_load_now {
            let code_info = self
                .database
                .code_info(code_id)
                .ok_or_else(|| anyhow!("Code info for code {code_id} is missing"))?;

            codes_futures.push(
                read_code_from_tx_hash(
                    self.blobs_reader.clone(),
                    code_id,
                    code_info.timestamp,
                    code_info.tx_hash,
                    None,
                )
                .boxed(),
            );
        }

        for res in future::join_all(codes_futures).await {
            let (code_id, _, code) = res?;
            self.database.set_original_code(code_id, code.as_slice());
        }

        for hash in chain.iter().rev() {
            let block_header = self
                .database
                .block_header(*hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            // Setting block as synced means: all on-chain data for this block is loaded and at least all positive validated codes are loaded.
            self.database.set_block_is_synced(*hash);

            self.database
                .set_latest_synced_block_height(block_header.height);
        }

        Ok((
            chain_head.hash.0.into(),
            codes_to_load_later.into_iter().collect(),
        ))
    }
}
