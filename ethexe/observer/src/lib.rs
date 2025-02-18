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
use anyhow::{Context as _, Result};
use ethexe_common::{
    events::{BlockEvent, RouterEvent},
    BlockData,
};
use ethexe_db::BlockHeader;
use ethexe_signer::Address;
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::{CodeId, H256};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

mod blobs;
mod observer;
mod query;

#[cfg(test)]
mod tests;

pub use blobs::*;
pub use observer::*;
pub use query::*;

type BlobDownloadFuture = BoxFuture<'static, Result<(CodeId, u64, Vec<u8>)>>;
type BlockFuture = BoxFuture<'static, Result<(H256, BlockHeader, Vec<BlockEvent>)>>;

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
    Block(BlockData),
}

pub struct ObserverService {
    blobs: Arc<dyn BlobReader>,
    provider: RootProvider,
    subscription: Subscription<Header>,

    router: Address,

    last_block_number: u32,

    headers_stream: SubscriptionStream<Header>,
    block_future: Option<BlockFuture>,

    codes_futures: FuturesUnordered<BlobDownloadFuture>,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.block_future.is_none() {
            if let Poll::Ready(res) = self.headers_stream.poll_next_unpin(cx) {
                if let Some(header) = res {
                    self.block_future = Some(Box::pin(Self::get_block(
                        header,
                        self.provider.clone(),
                        self.router,
                    )));
                } else {
                    // TODO: test resubscribe works.
                    log::warn!("Alloy headers stream ended, resubscribing");
                    self.headers_stream = self.subscription.resubscribe().into_stream();
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        }

        if let Some(fut) = self.block_future.as_mut() {
            if let Poll::Ready(res) = fut.poll_unpin(cx) {
                let event =
                    res.map(|(hash, header, events)| self.handle_stream_next(hash, header, events));

                self.block_future = None;

                return Poll::Ready(Some(event));
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
    pub async fn new(config: &EthereumConfig) -> Result<Self> {
        let blobs = Arc::new(
            ConsensusLayerBlobReader::new(&config.rpc, &config.beacon_rpc, config.block_time)
                .await
                .context("failed to create blob reader")?,
        );

        Self::new_with_blobs(config, blobs).await
    }

    pub async fn new_with_blobs(
        config: &EthereumConfig,
        blobs: Arc<dyn BlobReader>,
    ) -> Result<Self> {
        let provider = ProviderBuilder::default()
            .on_builtin(&config.rpc)
            .await
            .context("failed to create ethereum provider")?;

        let subscription = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?;

        let headers_stream = subscription.resubscribe().into_stream();

        Ok(Self {
            blobs,
            provider,
            subscription,
            router: config.router_address,
            last_block_number: 0,
            headers_stream,
            block_future: None,
            codes_futures: FuturesUnordered::new(),
        })
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

    pub fn lookup_code(&mut self, code_id: CodeId, timestamp: u64, tx_hash: H256) {
        self.codes_futures.push(Box::pin(read_code_from_tx_hash(
            self.blobs.clone(),
            code_id,
            timestamp,
            tx_hash,
            Some(3),
        )));
    }

    async fn get_block(
        header: Header,
        provider: RootProvider,
        router: Address,
    ) -> Result<(H256, BlockHeader, Vec<BlockEvent>)> {
        let hash = (*header.hash).into();
        let parent_hash = (*header.parent_hash).into();
        let block_number = header.number as u32;
        let block_timestamp = header.timestamp;

        let header = BlockHeader {
            height: block_number,
            timestamp: block_timestamp,
            parent_hash,
        };

        read_block_events(hash, &provider, router.0.into())
            .await
            .map(|events| (hash, header, events))
    }

    fn handle_stream_next(
        &mut self,
        hash: H256,
        header: BlockHeader,
        events: Vec<BlockEvent>,
    ) -> ObserverEvent {
        // TODO (breathx): set in db?
        log::trace!("Received block: {hash:?}");

        self.last_block_number = header.height;

        // TODO: replace me with proper processing of all events, including commitments.
        for event in &events {
            if let BlockEvent::Router(RouterEvent::CodeValidationRequested {
                code_id,
                timestamp,
                tx_hash,
            }) = event
            {
                self.lookup_code(*code_id, *timestamp, *tx_hash);
            }
        }

        ObserverEvent::Block(BlockData {
            hash,
            header,
            events,
        })
    }
}

impl Clone for ObserverService {
    fn clone(&self) -> Self {
        let subscription = self.subscription.resubscribe();
        let headers_stream = subscription.resubscribe().into_stream();

        Self {
            blobs: self.blobs.clone(),
            provider: self.provider.clone(),
            subscription,
            router: self.router,
            last_block_number: self.last_block_number,
            headers_stream,
            block_future: None,
            codes_futures: FuturesUnordered::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserverStatus {
    pub eth_best_height: u32,
    pub pending_codes: usize,
}
