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
    primitives::bytes::buf::Chain,
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::{RpcError, TransportErrorKind},
};
use anyhow::{anyhow, Context as _, Result};
use ethexe_blob_loader::{
    blobs::{BlobData, BlobReader},
    utils::read_code_from_tx_hash,
};
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
    collections::{HashSet, VecDeque},
    fmt,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use sync::{ChainSync, ChainSyncState};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

mod sync;

#[cfg(test)]
mod tests;

type BlobDownloadFuture = BoxFuture<'static, Result<BlobData>>;
type SyncFuture = BoxFuture<'static, Result<ObserverEvent>>;
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
pub enum ObserverEvent {
    Blob(BlobData),
    Block(SimpleBlockData),
    BlockSynced(BlockSyncedData),
    RequestLoadBlobs(Vec<CodeId>),
}

impl fmt::Debug for ObserverEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObserverEvent::Blob(data) => data.fmt(f),
            ObserverEvent::Block(data) => f.debug_tuple("Block").field(data).finish(),
            ObserverEvent::BlockSynced(hash) => f.debug_tuple("BlockSynced").field(hash).finish(),
            ObserverEvent::RequestLoadBlobs(codes) => {
                f.debug_tuple("RequestLoadBlobs").field(codes).finish()
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
}

// TODO #4552: make tests for observer service
pub struct ObserverService {
    provider: RootProvider,
    db: Database,
    // TODO: remove `blobs_reader` from `ObserverService` struct
    blobs_reader: Box<dyn BlobReader>,
    config: RuntimeConfig,

    chain_sync: ChainSync,
    codes_receiver: UnboundedReceiver<Vec<CodeId>>,
    loaded_blobs: VecDeque<BlobData>,

    // config: RuntimeConfig,
    last_block_number: u32,
    headers_stream: SubscriptionStream<Header>,

    // TODO: remove block_sync_queue
    block_sync_queue: VecDeque<Header>,

    // sync_future: Option<SyncFuture>,

    // TODO: remove `codes_futures` also
    codes_futures: FuturesUnordered<BlobDownloadFuture>,

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
            self.block_sync_queue.push_back(header.clone());
            self.chain_sync.pending_blocks.push_front(header);

            return Poll::Ready(Some(Ok(ObserverEvent::Block(data))));
        }

        if let Some(blob_data) = self.loaded_blobs.pop_back() {
            return Poll::Ready(Some(Ok(ObserverEvent::Blob(blob_data))));
        }

        if let Poll::Ready(Some(codes)) = self.codes_receiver.poll_recv(cx) {
            if !codes.is_empty() {
                return Poll::Ready(Some(Ok(ObserverEvent::RequestLoadBlobs(codes))));
            }
        }

        if let Poll::Ready(result) = self.chain_sync.poll_unpin(cx) {
            let event = result.map(|data| ObserverEvent::BlockSynced(data));
            return Poll::Ready(Some(event));
        }

        // if self.sync_future.is_none() {
        //     if let Some(header) = self.block_sync_queue.pop_front() {
        // let sync = ChainSync {
        //     provider: self.provider.clone(),
        //     db: self.db.clone(),
        //     blobs_reader: self.blobs_reader.clone(),
        //     config: self.config.clone(),
        // };
        //         self.sync_future = Some(Box::pin(sync.sync(header)));
        //     }
        // }

        // if let Some(fut) = self.sync_future.as_mut() {
        //     if let Poll::Ready(event) = fut.poll_unpin(cx) {
        //         self.sync_future = None;

        // let res = res.map(|(hash, codes)| {
        //     for (code_id, code_info) in codes {
        //         self.lookup_code(code_id, code_info.timestamp, code_info.tx_hash);
        //     }

        //     ObserverEvent::BlockSynced(hash)
        // });
        //         return Poll::Ready(Some(event));
        //     }
        // }

        // if let Poll::Ready(Some(res)) = self.codes_futures.poll_next_unpin(cx) {
        //     return Poll::Ready(Some(res.map(ObserverEvent::Blob)));
        // }

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
        blobs_reader: Box<dyn BlobReader>,
    ) -> Result<Self> {
        let EthereumConfig {
            rpc,
            router_address,
            block_time,
            ..
        } = eth_cfg;

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

        let runtime_cfg = RuntimeConfig {
            router_address: *router_address,
            wvara_address,
            max_sync_depth,
            batched_sync_depth: 2,
            block_time: *block_time,
        };

        let (s, r) = unbounded_channel();

        Ok(Self {
            provider: provider.clone(),
            db: db.clone(),
            blobs_reader: blobs_reader.clone(),
            config: runtime_cfg.clone(),
            chain_sync: ChainSync {
                blobs_reader,
                db,
                config: runtime_cfg,
                provider: provider,
                codes_sender: s,
                state: ChainSyncState::WaitingForBlock,
                load_chain_fut: None,
                finalize_sync_fut: None,
                codes_to_wait: None,
                chain: None,
                loaded_codes: HashSet::new(),
                pending_blocks: VecDeque::new(),
            },
            codes_receiver: r,
            loaded_blobs: VecDeque::new(),
            last_block_number: 0,
            subscription_future: None,
            headers_stream,
            codes_futures: Default::default(),
            block_sync_queue: Default::default(),
            // sync_future: Default::default(),
        })
    }

    // TODO #4563: this is a temporary solution.
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
        db.set_block_codes_queue(genesis_block_hash, Default::default());
        db.set_previous_not_empty_block(genesis_block_hash, H256::zero());
        db.set_block_program_states(genesis_block_hash, Default::default());
        db.set_block_schedule(genesis_block_hash, Default::default());
        db.set_block_outcome(genesis_block_hash, Default::default());

        db.set_latest_computed_block(genesis_block_hash, genesis_header);

        db.set_block_computed(genesis_block_hash);

        Ok(())
    }

    pub fn receive_loaded_blob(&mut self, blob_data: BlobData) -> Result<()> {
        // let blob_info = self
        //     .db
        //     .code_blob_info()
        //     .ok_or(anyhow!("Expect CodeInfo for {code_id} exists in database"))?;

        self.chain_sync.receive_loaded_code(blob_data.code_id);
        self.loaded_blobs.push_front(blob_data);
        Ok(())
    }

    async fn mark_blob_loaded() -> Result<()> {
        todo!();
    }

    // TODO: think about removing this
    pub fn provider(&self) -> &RootProvider {
        &self.provider
    }

    pub fn status(&self) -> ObserverStatus {
        ObserverStatus {
            eth_best_height: self.last_block_number,
            pending_codes: self.codes_futures.len(),
        }
    }

    pub async fn finalize_chain_head(&mut self, codes: HashSet<CodeId>) {
        todo!()
    }

    pub fn block_time_secs(&self) -> u64 {
        self.config.block_time.as_secs()
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
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserverStatus {
    pub eth_best_height: u32,
    pub pending_codes: usize,
}
