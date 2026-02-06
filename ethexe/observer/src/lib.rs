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

use crate::utils::EthereumBlockLoader;
use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::TransportResult,
};
use anyhow::{Context as _, Result, anyhow};
use ethexe_common::{
    Address, BlockHeader, ProtocolTimelines, SimpleBlockData, db::ConfigStorageRO,
};
use ethexe_db::Database;
use ethexe_ethereum::router::RouterQuery;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture, stream::FusedStream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};
use sync::ChainSync;

mod sync;
pub mod utils;

#[cfg(test)]
mod tests;

type HeadersSubscriptionFuture = BoxFuture<'static, TransportResult<Subscription<Header>>>;

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserverEvent {
    Block(SimpleBlockData),
    BlockSynced(H256),
}

#[derive(Clone, Debug)]
struct RuntimeConfig {
    /// Protocol timelines.
    timelines: ProtocolTimelines,
    /// Address of the Router contract.
    router_address: Address,
    /// Address of the Middleware contract.
    middleware_address: Address,
    /// Maximum depth of blocks to sync.
    max_sync_depth: u32,
    /// If block sync depth is greater than this value, blocks are synced in batches of this size.
    /// Must be greater than 1.
    batched_sync_depth: u32,
    /// Number of blocks after which election timestamp is considered finalized.
    finalization_period_blocks: u64,
}

// TODO #4552: make tests for observer service
pub struct ObserverService {
    provider: RootProvider,
    config: RuntimeConfig,
    chain_sync: ChainSync,

    last_block_number: u32,
    headers_stream: SubscriptionStream<Header>,

    block_sync_queue: VecDeque<Header>,
    sync_future: Option<BoxFuture<'static, Result<H256>>>,
    subscription_future: Option<HeadersSubscriptionFuture>,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If subscription stream finished working, a new subscription is requested to be created.
        // The subscription creation request is a future itself, and it is polled here. If it's ready,
        // a new stream from it is created and used further to poll the next header.
        if let Some(future) = self.subscription_future.as_mut() {
            match ready!(future.as_mut().poll(cx)) {
                Ok(subscription) => self.headers_stream = subscription.into_stream(),
                Err(e) => {
                    return Poll::Ready(Some(Err(anyhow!(
                        "failed to create new headers stream: {e}"
                    ))));
                }
            }
        }

        if let Poll::Ready(res) = self.headers_stream.poll_next_unpin(cx) {
            let Some(header) = res else {
                log::warn!("Alloy headers stream ended. Creating a new one...");

                // TODO #4568: test creating a new subscription in case when Receiver becomes invalid
                let provider = self.provider().clone();
                let _fut = provider.get_block_by_number(alloy::eips::BlockNumberOrTag::Earliest);
                self.subscription_future = Some(provider.subscribe_blocks().into_future());

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
            ..
        } = eth_cfg;

        let router_query = RouterQuery::new(rpc, *router_address).await?;
        let middleware_address = router_query.middleware_address().await?;

        let provider = ProviderBuilder::default()
            .connect(rpc)
            .await
            .context("failed to create ethereum provider")?;

        let headers_stream = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?
            .into_stream();

        let config = RuntimeConfig {
            timelines: db.config().timelines,
            router_address: *router_address,
            middleware_address,
            max_sync_depth,
            // TODO #4562: make this configurable.
            batched_sync_depth: 2,
            // TODO #4562: make this configurable, since different networks may have different finalization periods.
            finalization_period_blocks: 64,
        };

        let chain_sync = ChainSync::new(db, config.clone(), provider.clone());

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

    pub fn provider(&self) -> &RootProvider {
        &self.provider
    }

    pub fn last_block_number(&self) -> u32 {
        self.last_block_number
    }

    pub fn block_loader(&self) -> EthereumBlockLoader {
        EthereumBlockLoader::new(self.provider.clone(), self.config.router_address)
    }

    pub fn router_query(&self) -> RouterQuery {
        RouterQuery::from_provider(self.config.router_address, self.provider.clone())
    }
}
