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
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::{RpcError, TransportErrorKind},
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
type HeadersSubscriptionFuture =
    BoxFuture<'static, std::result::Result<Subscription<Header>, RpcError<TransportErrorKind>>>;

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

// TODO #4552: make tests for observer service
pub struct ObserverService {
    provider: RootProvider,
    db: Database,
    // TODO #4561: consider to make clone_boxed/clone for BlobRead, in order to avoid redundant Arc usage.
    blobs_reader: Arc<dyn BlobReader>,

    config: RuntimeConfig,
    last_block_number: u32,
    headers_stream: SubscriptionStream<Header>,
    block_sync_queue: VecDeque<Header>,

    sync_future: Option<SyncFuture>,
    codes_futures: FuturesUnordered<BlobDownloadFuture>,
    subscription_future: Option<HeadersSubscriptionFuture>,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If subscription stream finished working, a new subscription is requested to be created.
        // The subscription creation request is a future itself, and it is polled here. If it's ready,
        // a new stream from it is created and used further to poll the next header.
        if let Some(future) = self.subscription_future.as_mut() {
            match future.poll_unpin(cx) {
                Poll::Ready(Ok(subscription)) => self.headers_stream = subscription.into_stream(),
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Some(Err(anyhow!(
                        "failed to create new headers stream: {e}"
                    ))))
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        if let Poll::Ready(res) = self.headers_stream.poll_next_unpin(cx) {
            let Some(header) = res else {
                log::warn!("Alloy headers stream ended. Creating a new one...");

                // TODO #4568: test creating a new subscription in case when Receiver becomes invalid
                let provider = self.provider().clone();
                self.subscription_future =
                    Some(Box::pin(async move { provider.subscribe_blocks().await }));

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
        // TODO #4561: blobs reader should be provided by the caller always.
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

        Self::pre_process_genesis_for_db(&db, &provider, &router_query).await?;

        let headers_stream = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?
            .into_stream();

        Ok(Self {
            provider,
            db,
            blobs_reader,
            config: RuntimeConfig {
                router_address: *router_address,
                wvara_address,
                max_sync_depth,
                // TODO #4562: make this configurable. Important: must be greater than 1.
                batched_sync_depth: 2,
                block_time: *block_time,
            },
            last_block_number: 0,
            subscription_future: None,
            headers_stream,
            codes_futures: Default::default(),
            block_sync_queue: Default::default(),
            sync_future: Default::default(),
        })
    }

    // Populates database with genesis block data.
    //
    // Basically, requests data for the block, which is considered to be a genesis block
    // inside the `Router` contract on Ethereum. The data is processed the following way:
    // - header is stored in the database
    // - events are set as empty
    // - block is set as synced
    // - block is set as computed
    // - block is set as latest synced block (it's height)
    // - block is set as latest computed block
    // - previous non-empty block for the genesis one is set to blake2b256(0)
    // - all the runtime storages related to the block (message queue, tasks schedule, codes queue) also programs states,
    //   and processing outcome (state transitions) are set to default (empty) values.
    //
    // If genesis block was computed earlier, this function returns immediately.
    // TODO #4563: this is a temporary solution.
    // Choose a better place for this, out of ObserverService.
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
        db.set_block_codes_queue(genesis_block_hash, Default::default());
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
