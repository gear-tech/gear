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

use crate::observer::{read_block_request_events, read_code_from_tx_hash};
use alloy::{
    providers::{Provider as _, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::BoxTransport,
};
use anyhow::{anyhow, Context as _, Result};
use ethexe_common::events::{BlockEvent, BlockRequestEvent, RouterEvent, RouterRequestEvent};
use ethexe_db::BlockHeader;
use ethexe_signer::Address;
use futures::{future::BoxFuture, stream::{FusedStream, FuturesUnordered}, Future, FutureExt, Stream, StreamExt};
use gprimitives::{CodeId, H256};
use observer::read_block_events;
use std::{future::IntoFuture, pin::{pin, Pin}, sync::Arc, task::{Context, Poll}, time::Duration};

type Provider = RootProvider<BoxTransport>;

mod blobs;
mod event;
mod observer;
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

    blocks_stream: SubscriptionStream<Header>,
    codes_futures: FuturesUnordered<BlobDownloadFuture>,
}

type BlobDownloadFuture = BoxFuture<'static, Result<(CodeId, Vec<u8>)>>;

impl Clone for ObserverService {
    fn clone(&self) -> Self {
        let subscription = self.subscription.resubscribe();
        let blocks_stream = subscription.resubscribe().into_stream();

        Self {
            blobs: self.blobs.clone(),
            provider: self.provider.clone(),
            subscription,
            router: self.router,
            last_block_number: self.last_block_number,
            blocks_stream,
            codes_futures: FuturesUnordered::new(),
        }
    }
}

// impl Stream for ObserverService {
//     type Item = Result<ObserverServiceEvent>;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         if let Poll::Ready(event) = pin!(self.next_event()).poll(cx) {
//             log::debug!("READY {event:?}");
//             Poll::Ready(Some(event))
//         } else {
//             cx.waker().wake_by_ref();
//             Poll::Ready(None)
//         }
//     }
// }

// impl FusedStream for ObserverService {
//     fn is_terminated(&self) -> bool {
//         false
//     }
// }

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

        Ok(Self {
            blobs,
            provider,
            subscription,
            router: config.router_address,
            last_block_number: 0,
            blocks_stream,
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

    pub async fn next(&mut self) -> Result<ObserverServiceEvent> {
        tokio::select! {
            header = self.blocks_stream.next() => {
                let header = header.ok_or_else(|| anyhow!("blocks stream closed"))?;

                let block_hash = (*header.hash).into();
                let parent_hash = (*header.parent_hash).into();
                let block_number = header.number as u32;
                let block_timestamp = header.timestamp;

                log::trace!("Received block: {block_hash:?}");

                self.last_block_number = block_number as u64;

                let header = BlockHeader {
                    height: block_number,
                    timestamp: block_timestamp,
                    parent_hash,
                };

                let events = read_block_events(block_hash, &self.provider, self.router.0.into())
                    .await
                    .map_err(|err| anyhow!("Failed to read events for block {block_hash:?}: {err}"))?;

                // TODO: replace me with proper processing of all events, including commitments.
                for event in &events {
                    if let BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, blob_tx_hash }) = event {
                        self.lookup_code(*code_id, *blob_tx_hash);
                    }
                }

                Ok(ObserverServiceEvent::Block(BlockData {
                    hash: block_hash,
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
