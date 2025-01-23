// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::observer::read_code_from_tx_hash;
use alloy::{
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::BoxTransport,
};
use anyhow::{Context as _, Result};
use ethexe_common::events::{BlockEvent, RouterEvent};
use ethexe_db::BlockHeader;
use ethexe_signer::Address;
use futures::{
    future::{BoxFuture, Future},
    ready,
    stream::{FusedStream, FuturesUnordered},
    Stream, StreamExt,
};
use gprimitives::{CodeId, H256};
use observer::read_block_events;
use std::{
    pin::{pin, Pin},
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

type Provider = RootProvider<BoxTransport>;

mod blobs;
mod event;
pub mod observer;
mod query;

pub use blobs::{BlobReader, ConsensusLayerBlobReader, MockBlobReader};
pub use event::{BlockData, Event, RequestBlockData, RequestEvent, SimpleBlockData};
pub use observer::{Observer, ObserverStatus};
pub use query::Query;

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
}

pub struct ObserverService {
    blobs: Arc<dyn BlobReader>,
    provider: Provider,
    subscription: Subscription<Header>,

    router: Address,

    last_block_number: u64,

    stream: Pin<Box<BlocksStream>>,
    codes_futures: FuturesUnordered<BlobDownloadFuture>,
}

type BlobDownloadFuture = BoxFuture<'static, Result<(CodeId, Vec<u8>)>>;
type BlocksStream = dyn Stream<Item = (H256, BlockHeader, Vec<BlockEvent>)> + Send;

impl Clone for ObserverService {
    fn clone(&self) -> Self {
        let subscription = self.subscription.resubscribe();
        let stream = subscription.resubscribe().into_stream();

        let stream = Self::events_all(stream, self.provider.clone(), self.router);

        Self {
            blobs: self.blobs.clone(),
            provider: self.provider.clone(),
            subscription,
            router: self.router,
            last_block_number: self.last_block_number,
            stream: Box::pin(stream),
            codes_futures: FuturesUnordered::new(),
        }
    }
}

impl Stream for ObserverService {
    type Item = Result<ObserverServiceEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let e = ready!(pin!(self.next_event()).poll(cx));
        Poll::Ready(Some(e))
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
        let provider = ProviderBuilder::new()
            .on_builtin(&config.rpc)
            .await
            .context("failed to create ethereum provider")?;

        let subscription = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?;

        let blocks_stream = subscription.resubscribe().into_stream();

        let stream = Box::pin(Self::events_all(
            blocks_stream,
            provider.clone(),
            config.router_address,
        ));

        Ok(Self {
            blobs,
            provider,
            subscription,
            router: config.router_address,
            last_block_number: 0,
            stream,
            codes_futures: FuturesUnordered::new(),
        })
    }

    pub fn provider(&self) -> &Provider {
        &self.provider
    }

    pub fn get_status(&self) -> ObserverStatus {
        ObserverStatus {
            eth_block_number: self.last_block_number,
            last_router_state: 0, // what is this?
            pending_upload_code: self.codes_futures.len() as u64,
        }
    }

    pub fn lookup_code(&mut self, code_id: CodeId, blob_tx_hash: H256) {
        self.codes_futures.push(Box::pin(read_code_from_tx_hash(
            self.blobs.clone(),
            code_id,
            blob_tx_hash,
            Some(3),
        )));
    }

    fn events_all(
        mut stream: SubscriptionStream<Header>,
        provider: Provider,
        router: Address,
    ) -> impl Stream<Item = (H256, BlockHeader, Vec<BlockEvent>)> {
        async_stream::stream! {
            while let Some(header) = stream.next().await {
                let hash = (*header.hash).into();
                let parent_hash = (*header.parent_hash).into();
                let block_number = header.number as u32;
                let block_timestamp = header.timestamp;

                let header = BlockHeader {
                    height: block_number,
                    timestamp: block_timestamp,
                    parent_hash,
                };

                let events = read_block_events(hash, &provider, router.0.into()).await.unwrap();

                yield (hash, header, events);
            }
        }
    }

    async fn next_event(&mut self) -> Result<ObserverServiceEvent> {
        tokio::select! {
            Some((hash, header, events)) = self.stream.next() => {
                // TODO (breathx): set in db?
                log::trace!("Received block: {hash:?}");

                self.last_block_number = header.height as u64;

                // TODO: replace me with proper processing of all events, including commitments.
                for event in &events {
                    if let BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, blob_tx_hash }) = event {
                        self.lookup_code(*code_id, *blob_tx_hash);
                    }
                }

                Ok(ObserverServiceEvent::Block(BlockData {
                    hash,
                    header,
                    events,
                }))
            },
            Some(res) = self.codes_futures.next() => {
                res.map(|(code_id, code)| ObserverServiceEvent::Blob { code_id, code })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum ObserverServiceEvent {
    Blob { code_id: CodeId, code: Vec<u8> },
    Block(BlockData),
}
