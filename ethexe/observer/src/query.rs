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

use crate::{BlobReader, Provider, MAX_QUERY_BLOCK_RANGE};
use alloy::{
    network::{Ethereum, Network},
    primitives::Address as AlloyAddress,
    providers::{Provider as _, ProviderBuilder},
    rpc::{client::BatchRequest, types::eth::BlockTransactionsKind},
};
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage},
    events::{BlockEvent, BlockRequestEvent, RouterEvent},
};
use ethexe_signer::Address;
use futures::future;
use gprimitives::{CodeId, H256};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

/// Height difference to start fast sync.
const DEEP_SYNC: u32 = 10;

#[derive(Clone)]
pub struct Query {
    database: Arc<dyn BlockMetaStorage>,
    provider: Provider,
    router_address: AlloyAddress,
    genesis_block_hash: H256,
    blob_reader: Arc<dyn BlobReader>,
    max_commitment_depth: u32,
}

impl Query {
    pub async fn new(
        database: Arc<dyn BlockMetaStorage>,
        ethereum_rpc: &str,
        router_address: Address,
        genesis_block_hash: H256,
        blob_reader: Arc<dyn BlobReader>,
        max_commitment_depth: u32,
    ) -> Result<Self> {
        let mut query = Self {
            database,
            provider: ProviderBuilder::default().on_builtin(ethereum_rpc).await?,
            router_address: AlloyAddress::new(router_address.0),
            genesis_block_hash,
            blob_reader,
            max_commitment_depth,
        };
        // Initialize the database for the genesis block
        query.init_genesis_block().await?;

        Ok(query)
    }

    async fn init_genesis_block(&mut self) -> Result<()> {
        let hash = self.genesis_block_hash;
        self.database
            .set_block_commitment_queue(hash, Default::default());
        self.database
            .set_previous_committed_block(hash, H256::zero());
        self.database.set_block_end_state_is_valid(hash, true);
        self.database.set_block_is_empty(hash, true);
        self.database
            .set_block_end_program_states(hash, Default::default());
        self.database
            .set_block_end_schedule(hash, Default::default());

        // set latest valid if empty.
        if self.database.latest_valid_block().is_none() {
            let genesis_header = self.get_block_header_meta(hash).await?;
            self.database.set_latest_valid_block(hash, genesis_header);
        }

        Ok(())
    }

    async fn get_committed_blocks(&mut self, block_hash: H256) -> Result<BTreeSet<H256>> {
        // TODO (breathx): optimize me ASAP.
        Ok(self
            .get_block_events(block_hash)
            .await?
            .into_iter()
            .filter_map(|event| match event {
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => Some(hash),
                _ => None,
            })
            .collect())
    }

    async fn batch_get_block_headers(
        provider: Provider,
        database: Arc<dyn BlockMetaStorage>,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<(H256, BlockHeader)>> {
        log::debug!("Querying blocks from {from_block} to {to_block}");

        let mut batch = BatchRequest::new(provider.client());

        let handles: Vec<_> = (from_block..=to_block)
            .map(|bn| {
                batch
                    .add_call::<_, Option<<Ethereum as Network>::BlockResponse>>(
                        "eth_getBlockByNumber",
                        &(format!("0x{bn:x}"), false),
                    )
                    .expect("infallible")
            })
            .collect();

        batch.send().await?;

        let blocks: Vec<_> = future::join_all(handles).await;

        let mut res = Vec::with_capacity(blocks.len());

        for block in blocks {
            let block = block?.ok_or_else(|| anyhow!("Block not found"))?;
            let block_hash = H256(block.header.hash.0);

            let height = block.header.number as u32;
            let timestamp = block.header.timestamp;
            let parent_hash = H256(block.header.parent_hash.0);

            let header = BlockHeader {
                height,
                timestamp,
                parent_hash,
            };

            database.set_block_header(block_hash, header.clone());

            res.push((block_hash, header))
        }

        Ok(res)
    }

    /// Populate database with blocks using rpc provider.
    async fn load_chain_batch(
        &mut self,
        from_block: u32,
        to_block: u32,
    ) -> Result<HashMap<H256, BlockHeader>> {
        let total_blocks = to_block.saturating_sub(from_block) + 1;

        log::info!("Starting to load {total_blocks} blocks from {from_block} to {to_block}");

        let headers_handles: Vec<_> = (from_block..=to_block)
            .step_by(MAX_QUERY_BLOCK_RANGE)
            .map(|start| {
                let end = (start + MAX_QUERY_BLOCK_RANGE as u32 - 1).min(to_block);

                let provider = self.provider.clone();
                let database = self.database.clone();

                tokio::spawn(async move {
                    Self::batch_get_block_headers(provider, database, start as u64, end as u64)
                        .await
                })
            })
            .collect();

        let headers_fut = future::join_all(headers_handles);

        let events_fut = crate::read_block_request_events_batch(
            from_block,
            to_block,
            &self.provider,
            self.router_address,
        );

        let (headers_batches, maybe_events) = future::join(headers_fut, events_fut).await;
        let mut events = maybe_events?;

        let mut res = HashMap::with_capacity(total_blocks as usize);

        for batch in headers_batches {
            let batch = batch??;

            for (hash, header) in batch {
                self.database
                    .set_block_events(hash, events.remove(&hash).unwrap_or_default());

                res.insert(hash, header);
            }
        }

        log::trace!("{} blocks loaded", res.len());

        Ok(res)
    }

    pub async fn get_last_committed_chain(&mut self, block_hash: H256) -> Result<Vec<H256>> {
        let current_block = self.get_block_header_meta(block_hash).await?;
        let latest_valid_block_height = self
            .database
            .latest_valid_block()
            .map(|(_, header)| header.height)
            .expect("genesis by default; qed");

        if current_block.height >= latest_valid_block_height
            && (current_block.height - latest_valid_block_height) >= self.max_commitment_depth
        {
            return Err(anyhow!(
                "Too deep chain: Current block height: {}, Latest valid block height: {}, Max depth: {}",
                current_block.height,
                latest_valid_block_height,
                self.max_commitment_depth
            ));
        }

        // Determine if deep sync is needed
        let is_deep_sync = {
            // Current block can be lower than latest valid due to reorgs.
            let block_diff = current_block
                .height
                .saturating_sub(latest_valid_block_height);
            block_diff > DEEP_SYNC
        };

        let mut chain = Vec::new();
        let mut headers_map = HashMap::new();

        let committed_blocks = crate::read_committed_blocks_batch(
            latest_valid_block_height + 1,
            current_block.height,
            &self.provider,
            self.router_address,
        )
        .await?;

        if is_deep_sync {
            // Load blocks in batch from provider by numbers.
            headers_map = self
                .load_chain_batch(latest_valid_block_height + 1, current_block.height)
                .await?;
        }

        // Continue loading chain by parent hashes from the current block to the latest valid block.
        let mut hash = block_hash;

        while hash != self.genesis_block_hash {
            // If the block's end state is valid, set it as the latest valid block
            if self
                .database
                .block_end_state_is_valid(hash)
                .unwrap_or(false)
            {
                let header = match headers_map.get(&hash) {
                    Some(header) => header.clone(),
                    None => self.get_block_header_meta(hash).await?,
                };

                self.database.set_latest_valid_block(hash, header);

                log::trace!("Nearest valid in db block found: {hash}");
                break;
            }

            log::trace!("Include block {hash} in chain for processing");
            chain.push(hash);

            // Fetch parent hash from headers_map or database
            hash = match headers_map.get(&hash) {
                Some(header) => header.parent_hash,
                None => self.get_block_parent_hash(hash).await?,
            };
        }

        let mut actual_commitment_queue: VecDeque<H256> = self
            .database
            .block_commitment_queue(hash)
            .ok_or_else(|| {
                anyhow!(
                    "Commitment queue not found for block {hash}, possible database inconsistency."
                )
            })?
            .into_iter()
            .filter(|hash| !committed_blocks.contains(hash))
            .collect();

        let Some(oldest_not_committed_block) = actual_commitment_queue.pop_front() else {
            // All blocks before nearest valid block are committed,
            // so we need to execute all blocks from valid to current.
            return Ok(chain);
        };

        while hash != oldest_not_committed_block {
            log::trace!("Include block {hash} in chain for processing");
            chain.push(hash);

            hash = self.get_block_parent_hash(hash).await?;
        }

        log::trace!("Oldest not committed block reached: {}", hash);
        chain.push(hash);
        Ok(chain)
    }

    pub async fn propagate_meta_for_block(&mut self, block_hash: H256) -> Result<()> {
        let parent = self.get_block_parent_hash(block_hash).await?;

        if !self
            .database
            .block_end_state_is_valid(parent)
            .unwrap_or(false)
        {
            return Err(anyhow!("parent block is not valid for block {block_hash}"));
        }

        // Propagate program state hashes
        let program_state_hashes = self
            .database
            .block_end_program_states(parent)
            .ok_or_else(|| anyhow!("parent block end states not found"))?;
        self.database
            .set_block_start_program_states(block_hash, program_state_hashes);

        // Propagate scheduled tasks
        let schedule = self
            .database
            .block_end_schedule(parent)
            .ok_or_else(|| anyhow!("parent block schedule not found"))?;
        self.database.set_block_start_schedule(block_hash, schedule);

        // Propagate `wait for commitment` blocks queue
        let queue = self
            .database
            .block_commitment_queue(parent)
            .ok_or_else(|| anyhow!("parent block commitment queue not found"))?;
        let committed_blocks = self.get_committed_blocks(block_hash).await?;
        let current_queue = queue
            .into_iter()
            .filter(|hash| !committed_blocks.contains(hash))
            .collect();
        self.database
            .set_block_commitment_queue(block_hash, current_queue);

        // Propagate prev commitment (prev not empty block hash or zero for genesis).
        if self
            .database
            .block_is_empty(parent)
            .ok_or_else(|| anyhow!("Cannot identify whether parent is empty"))?
        {
            let parent_prev_commitment = self
                .database
                .previous_committed_block(parent)
                .ok_or_else(|| anyhow!("parent block prev commitment not found"))?;
            self.database
                .set_previous_committed_block(block_hash, parent_prev_commitment);
        } else {
            self.database
                .set_previous_committed_block(block_hash, parent);
        }

        Ok(())
    }

    pub async fn get_block_header_meta(&mut self, block_hash: H256) -> Result<BlockHeader> {
        match self.database.block_header(block_hash) {
            Some(meta) => Ok(meta),
            None => {
                let block = self
                    .provider
                    .get_block_by_hash(block_hash.0.into(), BlockTransactionsKind::Hashes)
                    .await?
                    .ok_or_else(|| anyhow!("Block not found"))?;

                let height = u32::try_from(block.header.number).unwrap_or_else(|err| {
                    unreachable!("Ethereum block number not fit in u32: {err}")
                });
                let timestamp = block.header.timestamp;
                let parent_hash = H256(block.header.parent_hash.0);

                let meta = BlockHeader {
                    height,
                    timestamp,
                    parent_hash,
                };

                self.database.set_block_header(block_hash, meta.clone());

                // Populate block events in db.
                let events = crate::read_block_request_events(
                    block_hash,
                    &self.provider,
                    self.router_address,
                )
                .await?;
                self.database.set_block_events(block_hash, events);

                Ok(meta)
            }
        }
    }

    pub async fn get_block_parent_hash(&mut self, block_hash: H256) -> Result<H256> {
        Ok(self.get_block_header_meta(block_hash).await?.parent_hash)
    }

    pub async fn get_block_events(&mut self, block_hash: H256) -> Result<Vec<BlockEvent>> {
        crate::read_block_events(block_hash, &self.provider, self.router_address).await
    }

    pub async fn get_block_request_events(
        &mut self,
        block_hash: H256,
    ) -> Result<Vec<BlockRequestEvent>> {
        if let Some(events) = self.database.block_events(block_hash) {
            return Ok(events);
        }

        let events =
            crate::read_block_request_events(block_hash, &self.provider, self.router_address)
                .await?;
        self.database.set_block_events(block_hash, events.clone());

        Ok(events)
    }

    pub async fn download_code(
        &self,
        expected_code_id: CodeId,
        timestamp: u64,
        tx_hash: H256,
    ) -> Result<Vec<u8>> {
        let blob_reader = self.blob_reader.clone();
        let attempts = Some(3);

        crate::read_code_from_tx_hash(blob_reader, expected_code_id, timestamp, tx_hash, attempts)
            .await
            .map(|res| res.2)
    }
}
