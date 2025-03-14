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
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
};
use anyhow::{anyhow, Context as _, Result};
use ethexe_common::{db::OnChainStorage, SimpleBlockData};
use ethexe_db::{BlockHeader, CodeInfo, Database};
use ethexe_ethereum::router::RouterQuery;
use ethexe_signer::Address;
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::VecDeque,
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use sync::ChainSync;
use utils::*;

mod blobs;
mod sync;
mod utils;

#[cfg(test)]
mod tests;

pub use blobs::*;

type BlobDownloadFuture = BoxFuture<'static, Result<BlobData>>;
type SyncFuture = BoxFuture<'static, Result<(BlockSyncedData, Vec<(CodeId, CodeInfo)>)>>;

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSyncedData {
    pub block_hash: H256,
    pub validators: Vec<Address>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct BlobData {
    pub code_id: CodeId,
    pub timestamp: u64,
    pub code: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ObserverEvent {
    Blob(BlobData),
    Block(SimpleBlockData),
    BlockSynced(BlockSyncedData),
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

impl fmt::Debug for ObserverEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObserverEvent::Blob(data) => data.fmt(f),
            ObserverEvent::Block(data) => f.debug_tuple("Block").field(data).finish(),
            ObserverEvent::BlockSynced(hash) => f.debug_tuple("BlockSynced").field(hash).finish(),
        }
    }
}

#[derive(Clone, Debug)]
struct RuntimeConfig {
    router_address: Address,
    wvara_address: Address,
    max_sync_depth: u32,
    batched_sync_depth: u32,
    block_time: Duration,
}

// TODO (gsobol): make tests for observer service
pub struct ObserverService {
    provider: RootProvider,
    db: Database,
    // TODO (gsobol): consider to make clone_boxed/clone for BlobRead, in order to avoid redundant Arc usage.
    blobs_reader: Arc<dyn BlobReader>,
    subscription: Subscription<Header>,

    config: RuntimeConfig,

    last_block_number: u32,

    headers_stream: SubscriptionStream<Header>,
    block_sync_queue: VecDeque<Header>,
    sync_future: Option<SyncFuture>,
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
                    db: self.db.clone(),
                    blobs_reader: self.blobs_reader.clone(),
                    config: self.config.clone(),
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
            return Poll::Ready(Some(res.map(ObserverEvent::Blob)));
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
    pub async fn new(
        eth_cfg: &EthereumConfig,
        max_sync_depth: u32,
        db: Database,
        // TODO (gsobol): blobs reader should be provided by the caller always.
        blobs_reader: Option<Arc<dyn BlobReader>>,
    ) -> Result<Self> {
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

        let provider = ProviderBuilder::default()
            .connect(rpc)
            .await
            .context("failed to create ethereum provider")?;

        let subscription = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?;

        Self::pre_process_genesis_for_db(&db, &provider, &router_query).await?;

        let headers_stream = subscription.resubscribe().into_stream();

        Ok(Self {
            provider,
            db,
            blobs_reader,
            subscription,
            config: RuntimeConfig {
                router_address: *router_address,
                wvara_address,
                max_sync_depth,
                // TODO (gsobol): make this configurable. Important: must be greater than 1.
                batched_sync_depth: 2,
                block_time: *block_time,
            },
            last_block_number: 0,
            headers_stream,
            codes_futures: Default::default(),
            block_sync_queue: Default::default(),
            sync_future: Default::default(),
        })
    }

    // TODO (gsobol): this is a temporary solution.
    // Choose a better place for this, out of ObserverService.
    /// If genesis block is not yet fully setup in the database, we need to do it
    async fn pre_process_genesis_for_db(
        db: &Database,
        provider: &RootProvider,
        router_query: &RouterQuery,
    ) -> Result<()> {
        use ethexe_common::db::BlockMetaStorage;

        let genesis_block_hash = router_query.genesis_block_hash().await?;

        if db.block_computed(genesis_block_hash) {
            return Ok(());
        }

        let genesis_block = provider
            .get_block_by_hash(genesis_block_hash.0.into())
            .await?
            .ok_or_else(|| {
                anyhow!("Genesis block with hash {genesis_block_hash:?} not found by rpc")
            })?;

        let genesis_header = BlockHeader {
            height: genesis_block.header.number as u32,
            timestamp: genesis_block.header.timestamp,
            parent_hash: H256(genesis_block.header.parent_hash.0),
        };

        db.set_block_header(genesis_block_hash, genesis_header.clone());
        db.set_block_events(genesis_block_hash, &[]);

        db.set_latest_synced_block_height(genesis_header.height);
        db.set_block_is_synced(genesis_block_hash);

        db.set_block_commitment_queue(genesis_block_hash, Default::default());
        db.set_previous_not_empty_block(genesis_block_hash, H256::zero());
        db.set_block_program_states(genesis_block_hash, Default::default());
        db.set_block_schedule(genesis_block_hash, Default::default());
        db.set_block_outcome(genesis_block_hash, Default::default());

        db.set_latest_computed_block(genesis_block_hash, genesis_header);

        db.set_block_computed(genesis_block_hash);

        Ok(())
    }

    pub fn provider(&self) -> &RootProvider {
        &self.provider
    }

    pub fn status(&self) -> ObserverStatus {
        ObserverStatus {
            eth_best_height: self.last_block_number,
            pending_codes: self.codes_futures.len(),
        }
    }

    pub fn block_time_secs(&self) -> u64 {
        self.config.block_time.as_secs()
    }

    fn lookup_code(&mut self, code_id: CodeId, timestamp: u64, tx_hash: H256) {
        self.codes_futures
            .push(Box::pin(utils::read_code_from_tx_hash(
                self.blobs_reader.clone(),
                code_id,
                timestamp,
                tx_hash,
                Some(3),
            )));
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserverStatus {
    pub eth_best_height: u32,
    pub pending_codes: usize,
}
