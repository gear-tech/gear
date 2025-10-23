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

use crate::{
    RuntimeConfig,
    utils::{BlockLoader, EthereumBlockLoader},
};
use alloy::{providers::RootProvider, rpc::types::eth::Header};
use anyhow::{Result, anyhow};
use ethexe_common::{
    self, BlockData, BlockHeader, CodeBlobInfo, SimpleBlockData,
    db::{LatestDataStorageWrite, OnChainStorageWrite},
    events::{BlockEvent, RouterEvent},
    gear_core::pages::num_traits::Zero,
};
use ethexe_ethereum::router::RouterQuery;
use gprimitives::H256;
use nonempty::NonEmpty;
use std::collections::HashMap;

pub(crate) trait SyncDB: OnChainStorageWrite + LatestDataStorageWrite + Clone {}
impl<T: OnChainStorageWrite + LatestDataStorageWrite + Clone> SyncDB for T {}

// TODO #4552: make tests for ChainSync
#[derive(Clone)]
pub(crate) struct ChainSync<DB: SyncDB> {
    pub db: DB,
    pub config: RuntimeConfig,
    pub router_query: RouterQuery,
    pub block_loader: EthereumBlockLoader,
}

impl<DB: SyncDB> ChainSync<DB> {
    pub fn new(db: DB, config: RuntimeConfig, provider: RootProvider) -> Self {
        let router_query =
            RouterQuery::from_provider(config.router_address.0.into(), provider.clone());
        let block_loader =
            EthereumBlockLoader::new(provider, config.router_address, config.wvara_address);
        Self {
            db,
            config,
            router_query,
            block_loader,
        }
    }

    pub async fn sync(self, chain_head: Header) -> Result<H256> {
        let block = SimpleBlockData {
            hash: H256(chain_head.hash.0),
            header: BlockHeader {
                height: chain_head.number as u32,
                timestamp: chain_head.timestamp,
                parent_hash: H256(chain_head.parent_hash.0),
            },
        };

        let blocks_data = self.pre_load_data(&block.header).await?;
        let chain = self.load_chain(&block, blocks_data).await?;

        self.mark_chain_as_synced(chain.into_iter().rev())?;

        // NOTE: Set validators for the chain head block only.
        // It's useless to set validators for all synced blocks currently.
        self.propagate_validators(&block).await?;

        Ok(block.hash)
    }

    async fn load_chain(
        &self,
        block: &SimpleBlockData,
        mut blocks_data: HashMap<H256, BlockData>,
    ) -> Result<Vec<SimpleBlockData>> {
        let mut chain = Vec::new();

        let mut current_block_hash = block.hash;
        while !self.db.block_synced(current_block_hash) {
            let block_data = match blocks_data.remove(&current_block_hash) {
                Some(data) => data,
                None => {
                    self.block_loader
                        .load(
                            current_block_hash,
                            (current_block_hash == block.hash).then_some(block.header),
                        )
                        .await?
                }
            };

            if current_block_hash != block_data.hash {
                unreachable!(
                    "Expected data for block hash {current_block_hash}, got for {}",
                    block_data.hash
                );
            }

            for event in block_data.events.iter() {
                if let &BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id,
                    timestamp,
                    tx_hash,
                }) = event
                {
                    self.db
                        .set_code_blob_info(code_id, CodeBlobInfo { timestamp, tx_hash });
                }
            }

            self.db
                .set_block_header(current_block_hash, block_data.header);
            self.db
                .set_block_events(current_block_hash, &block_data.events);

            chain.push(SimpleBlockData {
                hash: current_block_hash,
                header: block_data.header,
            });

            current_block_hash = block_data.header.parent_hash;
        }

        Ok(chain)
    }

    async fn pre_load_data(&self, header: &BlockHeader) -> Result<HashMap<H256, BlockData>> {
        let Some(latest) = self.db.latest_data() else {
            log::warn!("latest data is not set in the database");
            return Ok(Default::default());
        };

        if header.height <= latest.synced_block_height {
            log::warn!(
                "Get a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest.synced_block_height
            );
            // Suppose here that all data is already in db.
            return Ok(Default::default());
        }

        if (header.height - latest.synced_block_height) >= self.config.max_sync_depth {
            // TODO (gsobol): return an event to notify about too deep chain.
            return Err(anyhow!(
                "Too much to sync: current block number: {}, Latest valid block number: {}, Max depth: {}",
                header.height,
                latest.synced_block_height,
                self.config.max_sync_depth
            ));
        }

        if header.height - latest.synced_block_height < self.config.batched_sync_depth {
            // No need to pre load data, because amount of blocks is small enough.
            return Ok(Default::default());
        }

        self.block_loader
            .load_many(latest.synced_block_height as u64..=header.height as u64)
            .await
    }

    // Propagate validators from the parent block. If start new era, fetch new validators from the router.
    async fn propagate_validators(&self, block: &SimpleBlockData) -> Result<()> {
        let validators = match self.db.block_validators(block.header.parent_hash) {
            Some(validators) if !self.should_fetch_validators(block.header)? => validators,
            _ => {
                let fetched_validators = self.router_query.validators_at(block.hash).await?;
                NonEmpty::from_vec(fetched_validators).ok_or(anyhow!(
                    "validator set is empty on router for block({})",
                    block.hash
                ))?
            }
        };
        self.db.set_block_validators(block.hash, validators.clone());
        Ok(())
    }

    fn mark_chain_as_synced(&self, chain: impl Iterator<Item = SimpleBlockData>) -> Result<()> {
        let mut head_height = None;
        for block in chain {
            self.db.set_block_synced(block.hash);
            head_height = Some(block.header.height);
        }

        if let Some(head_height) = head_height {
            self.db
                .mutate_latest_data(|data| data.synced_block_height = head_height)
                .ok_or_else(|| anyhow!("latest data is not set in db"))?;
        }

        Ok(())
    }

    /// NOTE: we don't need to fetch validators for block from zero era, because of
    /// it will be fetched in [`crate::ObserverService::pre_process_genesis_for_db`]
    fn should_fetch_validators(&self, chain_head: BlockHeader) -> Result<bool> {
        let chain_head_era = self.block_era_index(chain_head.timestamp);

        if chain_head_era.is_zero() {
            return Ok(false);
        }

        let parent = self.db.block_header(chain_head.parent_hash).ok_or(anyhow!(
            "header not found for block({:?})",
            chain_head.parent_hash
        ))?;

        let parent_era_index = self.block_era_index(parent.timestamp);
        Ok(chain_head_era > parent_era_index)
    }

    fn block_era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.config.genesis_timestamp) / self.config.era_duration
    }
}
