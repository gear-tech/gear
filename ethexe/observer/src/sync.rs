// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Implementation of the on-chain data synchronization.

use crate::{BlobData, BlobReader, BlockSyncedData, ObserverEvent, RuntimeConfig};
use alloy::{primitives::Address, providers::RootProvider, rpc::types::eth::Header};
use anyhow::{anyhow, Result};
use ethexe_blob_loader::utils::{load_block_data, load_blocks_data_batched};
use ethexe_common::{
    db::OnChainStorage,
    events::{BlockEvent, RouterEvent},
    BlockData,
};
use ethexe_db::{BlockHeader, CodeInfo, CodesStorage, Database};
use ethexe_ethereum::router::RouterQuery;
use futures::{
    future::BoxFuture,
    stream::{FuturesUnordered, Stream},
    FutureExt,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    task::Poll,
};
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

#[derive(Clone)]
pub(crate) enum ChainSyncState {
    WaitingForBlock,
    LoadingChain,
    WaitingForCodes,
    Finalize,
}

struct ChainFinalizer {
    pub router_address: Address,
    pub db: Database,
    pub provider: RootProvider,
}

struct ChainLoader {
    pub config: RuntimeConfig,
    pub db: Database,
    pub provider: RootProvider,
}

impl ChainLoader {
    pub async fn load(
        self,
        chain_head: Header,
    ) -> Result<(Vec<H256>, HashSet<CodeId>, Vec<CodeId>)> {
        let block: H256 = chain_head.hash.0.into();
        let header = BlockHeader {
            height: chain_head.number as u32,
            timestamp: chain_head.timestamp,
            parent_hash: H256(chain_head.parent_hash.0),
        };

        let blocks_data = self.pre_load_data(&header).await?;
        Ok(self.load_chain(block, header, blocks_data).await?)
    }

    async fn load_chain(
        &self,
        block: H256,
        header: BlockHeader,
        mut blocks_data: HashMap<H256, BlockData>,
    ) -> Result<(Vec<H256>, HashSet<CodeId>, Vec<CodeId>)> {
        let mut chain = Vec::new();

        // let mut codes_to_load = Vec::new();
        let mut codes_to_load_now = HashSet::new();
        let mut codes_to_load_later = HashSet::new();

        let mut hash = block;
        while !self.db.block_is_synced(hash) {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    load_block_data(
                        self.provider.clone(),
                        hash,
                        self.config.router_address,
                        self.config.wvara_address,
                        (hash == block).then_some(header.clone()),
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
                        self.db.set_code_blob_info(*code_id, code_info.clone());

                        if !self.db.original_code_exists(*code_id)
                            && !codes_to_load_now.contains(code_id)
                        {
                            // codes_to_load_later.insert(*code_id, code_info);
                            codes_to_load_later.insert(*code_id);
                        }
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                        if codes_to_load_later.contains(code_id) {
                            return Err(anyhow!("Code {code_id} is validated before requested"));
                        };

                        if !self.db.original_code_exists(*code_id) {
                            codes_to_load_now.insert(*code_id);
                        }
                    }
                    _ => {}
                }
            }

            let parent_hash = block_data.header.parent_hash;

            self.db.set_block_header(hash, block_data.header);
            self.db.set_block_events(hash, &block_data.events);

            chain.push(hash);

            hash = parent_hash;
        }

        codes_to_load_later.extend(codes_to_load_now.clone().into_iter());

        Ok((
            chain,
            codes_to_load_now,
            // codes_to_load_later.into_iter().collect(),
            codes_to_load_later.into_iter().collect(),
        ))
    }

    async fn pre_load_data(&self, header: &BlockHeader) -> Result<HashMap<H256, BlockData>> {
        let Some(latest_synced_block_height) = self.db.latest_synced_block_height() else {
            log::warn!("latest_synced_block_height is not set in the database");
            return Ok(Default::default());
        };

        if header.height <= latest_synced_block_height {
            log::warn!(
                "Get a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest_synced_block_height
            );
            // Suppose here that all data is already in db.
            return Ok(Default::default());
        }

        if (header.height - latest_synced_block_height) >= self.config.max_sync_depth {
            // TODO (gsobol): return an event to notify about too deep chain.
            return Err(anyhow!(
                    "Too much to sync: current block number: {}, Latest valid block number: {}, Max depth: {}",
                    header.height,
                    latest_synced_block_height,
                    self.config.max_sync_depth
                ));
        }

        if header.height - latest_synced_block_height < self.config.batched_sync_depth {
            // No need to pre load data, because amount of blocks is small enough.
            return Ok(Default::default());
        }

        load_blocks_data_batched(
            self.provider.clone(),
            latest_synced_block_height as u64,
            header.height as u64,
            self.config.router_address,
            self.config.wvara_address,
        )
        .await
    }
}

impl ChainFinalizer {
    pub async fn finalize(self, chain: Vec<H256>, block_hash: H256) -> Result<BlockSyncedData> {
        // NOTE: reverse order is important here, because by default chain was loaded in order from head to past.
        self.mark_chain_as_synced(chain.into_iter().rev());

        let validators = RouterQuery::from_provider(self.router_address, self.provider.clone())
            .validators_at(block_hash)
            .await?;

        Ok(BlockSyncedData {
            block_hash,
            validators,
        })
    }

    fn mark_chain_as_synced(&self, chain: impl Iterator<Item = H256>) {
        for hash in chain {
            let block_header = self
                .db
                .block_header(hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            self.db.set_block_is_synced(hash);

            self.db.set_latest_synced_block_height(block_header.height);
        }
    }
}

// TODO #4552: make tests for ChainSync
pub(crate) struct ChainSync {
    pub blobs_reader: Box<dyn BlobReader>,
    pub db: Database,
    pub config: RuntimeConfig,
    pub provider: RootProvider,

    pub codes_sender: UnboundedSender<Vec<CodeId>>,

    pub load_chain_fut:
        Option<BoxFuture<'static, Result<(Vec<H256>, HashSet<CodeId>, Vec<CodeId>)>>>,
    pub finalize_sync_fut: Option<BoxFuture<'static, Result<BlockSyncedData>>>,
    pub codes_to_wait: Option<HashSet<CodeId>>,
    pub chain: Option<Vec<H256>>,

    pub state: ChainSyncState,
    pub loaded_codes: HashSet<CodeId>,
    pub pending_blocks: VecDeque<Header>,
}

impl Future for ChainSync {
    type Output = Result<BlockSyncedData>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.state.clone() {
            ChainSyncState::WaitingForBlock => {
                log::info!("State: waiting for block, pending blocks len: {}", self.pending_blocks.len());
                if let Some(header) = self.pending_blocks.back() {
                    let chain_loader = ChainLoader {
                        config: self.config.clone(),
                        db: self.db.clone(),
                        provider: self.provider.clone(),
                    };
                    self.as_mut().load_chain_fut =
                        Some(Box::pin(chain_loader.load(header.clone())));
                    self.as_mut().state = ChainSyncState::LoadingChain;
                    cx.waker().wake_by_ref();
                }
                return Poll::Pending;
            }
            ChainSyncState::LoadingChain => {
                log::info!("State: loading chain");
                let result = self.load_chain_fut.as_mut().unwrap().poll_unpin(cx);
                match result {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => match result {
                        Ok((chain, codes_load_now, codes_to_load)) => {
                            if let Err(e) = self.codes_sender.send(codes_to_load) {
                                return Poll::Ready(Err(e.into()));
                            }
                            self.codes_to_wait = Some(codes_load_now);
                            self.chain = Some(chain);
                            self.load_chain_fut = None;
                            self.state = ChainSyncState::WaitingForCodes;
                            cx.waker().wake_by_ref();
                            //  (chain, codes_load_now));
                            return Poll::Pending;
                        }
                        Err(e) => return Poll::Ready(Err(e)),
                    },
                }
            }
            ChainSyncState::WaitingForCodes => {
                log::info!("State: waiting for codes");
                for code in self.loaded_codes.clone().into_iter() {
                    if self.codes_to_wait.as_ref().unwrap().contains(&code) {
                        self.codes_to_wait.as_mut().unwrap().remove(&code);
                        self.loaded_codes.remove(&code);
                    }
                }

                if self.codes_to_wait.as_ref().unwrap().is_empty() {
                    // TODO: remove unwrap
                    let chain_head = self.pending_blocks.pop_back().unwrap();
                    self.state = ChainSyncState::Finalize;

                    let chain_finalizer = ChainFinalizer {
                        router_address: self.config.router_address.0.into(),
                        db: self.db.clone(),
                        provider: self.provider.clone(),
                    };

                    self.finalize_sync_fut = Some(Box::pin(
                        chain_finalizer
                            .finalize(self.chain.clone().unwrap(), (*chain_head.hash).into()),
                    ));
                    cx.waker().wake_by_ref();
                }

                return Poll::Pending;
            }

            ChainSyncState::Finalize => {
                log::info!("State: finalizing");
                match self.finalize_sync_fut.as_mut().unwrap().poll_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(result) => {
                        self.finalize_sync_fut = None;
                        self.as_mut().state = ChainSyncState::WaitingForBlock;
                        // cx.waker().wake_by_ref();
                        return Poll::Ready(result);
                    }
                }
            }
        }
    }
}

impl ChainSync {
    pub fn receive_loaded_code(&mut self, code_id: CodeId) {
        self.loaded_codes.insert(code_id);
    }

    pub fn sync_chain_header(&mut self, header: Header) {
        self.pending_blocks.push_front(header);
    }
}
