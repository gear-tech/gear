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
    db::{OnChainStorageRO, OnChainStorageRW},
    events::{BlockEvent, RouterEvent},
};
use ethexe_db::Database;
use ethexe_ethereum::{
    middleware::{ElectionProvider, MiddlewareQuery},
    router::RouterQuery,
};
use gprimitives::H256;
use std::{collections::HashMap, ops::Add};

// TODO #4552: make tests for ChainSync
#[derive(Clone)]
pub(crate) struct ChainSync {
    pub db: Database,
    pub config: RuntimeConfig,
    pub router_query: RouterQuery,
    pub middleware_query: MiddlewareQuery,
    pub block_loader: EthereumBlockLoader,
}

impl ChainSync {
    pub fn new(db: Database, config: RuntimeConfig, provider: RootProvider) -> Self {
        let router_query =
            RouterQuery::from_provider(config.router_address.0.into(), provider.clone());
        let middleware_query =
            MiddlewareQuery::from_provider(config.middleware_address.0.into(), provider.clone());
        let block_loader = EthereumBlockLoader::new(provider, config.router_address);
        Self {
            db,
            config,
            router_query,
            middleware_query,
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

        self.ensure_validators(block).await?;

        let blocks_data = self.pre_load_data(&block.header).await?;
        let chain = self.load_chain(&block, blocks_data).await?;
        self.set_chain_in_db(chain.into_iter().rev())?;

        self.db.latest_data_mutate(|data| {
            data.synced_block = block;
        });

        Ok(block.hash)
    }

    async fn load_chain(
        &self,
        block: &SimpleBlockData,
        mut blocks_data: HashMap<H256, BlockData>,
    ) -> Result<Vec<BlockData>> {
        let mut chain = Vec::new();

        let mut current_block_hash = block.hash;
        while self.db.synced_block_read(current_block_hash).is_none() {
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

            current_block_hash = block_data.header.parent_hash;
            chain.push(block_data);
        }

        Ok(chain)
    }

    /// Loads blocks if there is a gap between the `header`'s height and the latest synced block height.
    async fn pre_load_data(&self, header: &BlockHeader) -> Result<HashMap<H256, BlockData>> {
        let latest_synced_block_height = self.db.latest_data_read().synced_block.header.height;

        if header.height <= latest_synced_block_height {
            tracing::warn!(
                "Got a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest_synced_block_height
            );

            // Suppose (but not rely on) here that all data is already in db.
            return Ok(Default::default());
        }

        if (header.height - latest_synced_block_height) >= self.config.max_sync_depth {
            return Err(anyhow!(
                "Too much to sync: current block number: {}, Latest synced block number: {}, Max depth: {}",
                header.height,
                latest_synced_block_height,
                self.config.max_sync_depth
            ));
        }

        if header.height - latest_synced_block_height < self.config.batched_sync_depth {
            // No need to pre load data, because amount of blocks is small enough.
            return Ok(Default::default());
        }

        self.block_loader
            .load_many(latest_synced_block_height as u64..=header.height as u64)
            .await
    }

    /// This function guarantees the next things:
    /// 1. if there is no validators for current era in database - it fetches them.
    /// 2. if the election result is `finalized` it requests for next era validators and sets them in database.
    ///
    /// See [`Self::election_timestamp_finalized`] for the our timestamp `finalization` rules.
    async fn ensure_validators(&self, data: SimpleBlockData) -> Result<()> {
        let timelines = self
            .db
            .protocol_timelines()
            .ok_or_else(|| anyhow!("protocol timelines not found in database"))?;
        let chain_head_era = timelines.era_from_ts(data.header.timestamp);

        // If we don't have validators for current era - set them.
        if self.db.validators(chain_head_era).is_none() {
            let validators = self.router_query.validators_at(data.hash).await?;
            self.db.set_validators(chain_head_era, validators);
        }

        // Fetch next era validators if timestamp `finalized` and we don't set them in database already.
        if let Some(election_ts) = self.election_timestamp_finalized(data.header)
            && self.db.validators(chain_head_era.add(1)).is_none()
        {
            let next_era_validators = self
                .middleware_query
                .make_election_at(election_ts, 10)
                .await?;
            self.db
                .set_validators(chain_head_era.add(1), next_era_validators);
        }

        Ok(())
    }

    fn set_chain_in_db(&self, chain: impl IntoIterator<Item = BlockData>) -> Result<()> {
        for data in chain {
            let block_hash = data.hash;
            self.db.synced_block_set(data)?;

            log::trace!(
                "✅ block {block_hash} synced, events: {:?}",
                self.db.block_events(block_hash)
            );
        }

        Ok(())
    }

    /// Function checks the `election_ts` in current era is `finalized` and if it's true then returns it.
    ///
    /// By `finalization` we mean the 64 blocks, because of it is closely to real finalization time and
    /// reorgs for 64 blocks can not happen.
    fn election_timestamp_finalized(&self, chain_head: BlockHeader) -> Option<u64> {
        let timelines = self.db.protocol_timelines()?;
        let election_ts =
            timelines.era_election_start_ts(timelines.era_from_ts(chain_head.timestamp));
        (chain_head.timestamp.saturating_sub(election_ts)
            > self.config.slot_duration_secs * self.config.finalization_period_blocks)
            .then_some(election_ts)
    }
}
