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

use crate::utils::load_block_data;
use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::{RpcError, TransportErrorKind},
};
use anyhow::{Context as _, Result, anyhow};
use ethexe_common::{
    Address, BlockData, BlockHeader, Digest, SimpleBlockData,
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
};
use ethexe_db::Database;
use ethexe_ethereum::router::RouterQuery;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture, stream::FusedStream};
use gprimitives::H256;
use nonempty::NonEmpty;
use std::{
    collections::VecDeque,
    fmt,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use sync::ChainSync;

mod sync;
mod utils;

#[cfg(test)]
mod tests;

type HeadersSubscriptionFuture =
    BoxFuture<'static, std::result::Result<Subscription<Header>, RpcError<TransportErrorKind>>>;

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ObserverEvent {
    Block(SimpleBlockData),
    BlockSynced(H256),
}

impl fmt::Debug for ObserverEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObserverEvent::Block(data) => f.debug_tuple("Block").field(data).finish(),
            ObserverEvent::BlockSynced(synced_block) => {
                f.debug_tuple("BlockSynced").field(synced_block).finish()
            }
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
    genesis_timestamp: u64,
    era_duration: u64,
}

// TODO #4552: make tests for observer service
pub struct ObserverService {
    provider: RootProvider,
    config: RuntimeConfig,
    chain_sync: ChainSync<Database>,

    last_block_number: u32,
    headers_stream: SubscriptionStream<Header>,

    block_sync_queue: VecDeque<Header>,
    sync_future: Option<BoxFuture<'static, Result<H256>>>,
    subscription_future: Option<HeadersSubscriptionFuture>,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(future) = self.subscription_future.as_mut() {
            match future.poll_unpin(cx) {
                Poll::Ready(Ok(subscription)) => self.headers_stream = subscription.into_stream(),
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Some(Err(anyhow!(
                        "failed to create new headers stream: {e}"
                    ))));
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
            self.block_sync_queue.push_front(header);

            return Poll::Ready(Some(Ok(ObserverEvent::Block(data))));
        }

        if self.sync_future.is_none()
            && let Some(header) = self.block_sync_queue.pop_back()
        {
            self.sync_future = Some(self.chain_sync.clone().sync(header).boxed());
        }

        if let Some(fut) = self.sync_future.as_mut()
            && let Poll::Ready(result) = fut.poll_unpin(cx)
        {
            self.sync_future = None;

            let maybe_event = result.map(ObserverEvent::BlockSynced);
            return Poll::Ready(Some(maybe_event));
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
    pub async fn new(eth_cfg: &EthereumConfig, max_sync_depth: u32, db: Database) -> Result<Self> {
        let EthereumConfig {
            rpc,
            router_address,
            block_time,
            ..
        } = eth_cfg;

        let router_query = RouterQuery::new(rpc, *router_address).await?;

        let wvara_address = Address(router_query.wvara_address().await?.0.0);

        let provider = ProviderBuilder::default()
            .connect(rpc)
            .await
            .context("failed to create ethereum provider")?;

        let genesis_header =
            Self::pre_process_genesis_for_db(&db, &provider, &router_query).await?;

        let timelines = router_query.timelines().await?;

        let headers_stream = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?
            .into_stream();

        let config = RuntimeConfig {
            router_address: *router_address,
            wvara_address,
            max_sync_depth,
            // TODO #4562: make this configurable. Important: must be greater than 1.
            batched_sync_depth: 2,
            block_time: *block_time,
            genesis_timestamp: genesis_header.timestamp,
            era_duration: timelines.era,
        };

        let chain_sync = ChainSync {
            db,
            provider: provider.clone(),
            config: config.clone(),
        };

        Ok(Self {
            provider,
            config,

            chain_sync,
            sync_future: None,
            block_sync_queue: VecDeque::new(),

            last_block_number: 0,
            subscription_future: None,
            headers_stream,
        })
    }

    // TODO #4563: this is a temporary solution.
    // Choose a better place for this, out of ObserverService.
    /// If genesis block is not yet fully setup in the database, we need to do it
    async fn pre_process_genesis_for_db<
        DB: BlockMetaStorageRead + BlockMetaStorageWrite + OnChainStorageRead + OnChainStorageWrite,
    >(
        db: &DB,
        provider: &RootProvider,
        router_query: &RouterQuery,
    ) -> Result<BlockHeader> {
        let genesis_block_hash = router_query.genesis_block_hash().await?;

        if db.block_meta(genesis_block_hash).computed {
            return db
                .block_header(genesis_block_hash)
                .ok_or(anyhow!("block header not found for {genesis_block_hash:?}"));
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

        let genesis_validators =
            NonEmpty::from_vec(router_query.validators_at(genesis_block_hash).await?)
                .ok_or(anyhow!("genesis validator set is empty"))?;

        db.set_block_header(genesis_block_hash, genesis_header.clone());
        db.set_block_events(genesis_block_hash, &[]);
        db.set_latest_synced_block_height(genesis_header.height);
        db.mutate_block_meta(genesis_block_hash, |meta| {
            meta.computed = true;
            meta.prepared = true;
            meta.synced = true;
            meta.last_committed_batch = Some(Digest([0; 32]));
            meta.last_committed_head = Some(genesis_block_hash);
        });

        db.set_block_codes_queue(genesis_block_hash, Default::default());
        db.set_block_program_states(genesis_block_hash, Default::default());
        db.set_block_schedule(genesis_block_hash, Default::default());
        db.set_block_outcome(genesis_block_hash, Default::default());
        db.set_latest_computed_block(genesis_block_hash, genesis_header.clone());
        db.set_validators(genesis_block_hash, genesis_validators);

        Ok(genesis_header)
    }

    pub fn provider(&self) -> &RootProvider {
        &self.provider
    }

    pub fn last_block_number(&self) -> u32 {
        self.last_block_number
    }

    pub fn block_time_secs(&self) -> u64 {
        self.config.block_time.as_secs()
    }

    pub fn load_block_data(&self, block: H256) -> impl Future<Output = Result<BlockData>> {
        load_block_data(
            self.provider.clone(),
            block,
            self.config.router_address,
            self.config.wvara_address,
            None,
        )
    }

    pub fn router_query(&self) -> RouterQuery {
        RouterQuery::from_provider(self.config.router_address.0.into(), self.provider.clone())
    }
}
