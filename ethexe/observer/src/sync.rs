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

use crate::{RuntimeConfig, utils};
use alloy::{providers::RootProvider, rpc::types::eth::Header};
use anyhow::{Result, anyhow};
use ethexe_common::{
    self, BlockData, BlockHeader, CodeBlobInfo,
    db::{LatestDataStorageRW, OnChainStorageRW},
    events::{BlockEvent, RouterEvent},
};
use ethexe_ethereum::{
    middleware::{ElectionProvider, MiddlewareQuery},
    router::RouterQuery,
};
use gprimitives::H256;
use std::{collections::HashMap, ops::Add};

pub(crate) trait SyncDB: OnChainStorageRW + LatestDataStorageRW + Clone {}
impl<T: OnChainStorageRW + LatestDataStorageRW + Clone> SyncDB for T {}

// TODO #4552: make tests for ChainSync
#[derive(Clone)]
pub(crate) struct ChainSync<DB: SyncDB> {
    pub db: DB,
    pub config: RuntimeConfig,
    pub provider: RootProvider,
}

impl<DB: SyncDB> ChainSync<DB> {
    pub async fn sync(self, chain_head: Header) -> Result<H256> {
        let block: H256 = chain_head.hash.0.into();
        let header = BlockHeader {
            height: chain_head.number as u32,
            timestamp: chain_head.timestamp,
            parent_hash: H256(chain_head.parent_hash.0),
        };

        let blocks_data = self.pre_load_data(&header).await?;
        let chain = self.load_chain(block, header, blocks_data).await?;

        self.ensure_validators(block, header).await?;
        self.mark_chain_as_synced(chain.into_iter().rev());

        Ok(block)
    }

    async fn load_chain(
        &self,
        block: H256,
        header: BlockHeader,
        mut blocks_data: HashMap<H256, BlockData>,
    ) -> Result<Vec<H256>> {
        let mut chain = Vec::new();

        let mut hash = block;
        while !self.db.block_synced(hash) {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    utils::load_block_data(
                        self.provider.clone(),
                        hash,
                        self.config.router_address,
                        self.config.wvara_address,
                        (hash == block).then_some(header),
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

            let parent_hash = block_data.header.parent_hash;

            self.db.set_block_header(hash, block_data.header);
            self.db.set_block_events(hash, &block_data.events);

            chain.push(hash);
            hash = parent_hash;
        }

        Ok(chain)
    }

    async fn pre_load_data(&self, header: &BlockHeader) -> Result<HashMap<H256, BlockData>> {
        let Some(latest) = self.db.latest_data() else {
            tracing::warn!("latest data is not set in the database");
            return Ok(Default::default());
        };

        if header.height <= latest.synced_block_height {
            tracing::warn!(
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

        utils::load_blocks_data_batched(
            self.provider.clone(),
            latest.synced_block_height as u64,
            header.height as u64,
            self.config.router_address,
            self.config.wvara_address,
        )
        .await
    }

    /// This function guarantees the next things:
    /// 1. if there is no validators for current era in database - it fetches them.
    /// 2. if the election result is `finalized` it requests for next era validators and sets them in database.
    ///
    /// See [`Self::election_timestamp_finalized`] for the our timestamp `finalization` rules.
    async fn ensure_validators(&self, block_hash: H256, header: BlockHeader) -> Result<()> {
        let timelines = self
            .db
            .protocol_timelines()
            .ok_or_else(|| anyhow!("protocol timelines not found in database"))?;
        let chain_head_era = timelines.era_from_ts(header.timestamp);

        // If we don't have validators for current era - set them.
        if self.db.validators(chain_head_era).is_none() {
            let router_query = RouterQuery::from_provider(
                self.config.router_address.into(),
                self.provider.clone(),
            );
            let validators = router_query.validators_at(block_hash).await?;
            self.db.set_validators(chain_head_era, validators);
        }

        // Fetch next era validators if timestamp `finalized` and we don't set them in database already.
        if let Some(election_ts) = self.election_timestamp_finalized(header)
            && self.db.validators(chain_head_era.add(1)).is_none()
        {
            let middleware_query =
                MiddlewareQuery::new(self.provider.clone(), self.config.middleware_address);
            let next_era_validators = middleware_query.make_election_at(election_ts, 10).await?;
            self.db
                .set_validators(chain_head_era.add(1), next_era_validators);
        }

        Ok(())
    }

    fn mark_chain_as_synced(&self, chain: impl Iterator<Item = H256>) {
        for hash in chain {
            let block_header = self
                .db
                .block_header(hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            self.db.set_block_synced(hash);

            let _ = self
                .db
                .mutate_latest_data(|data| data.synced_block_height = block_header.height)
                .ok_or_else(|| {
                    log::error!("Failed to update latest data for synced block {hash}");
                });
        }
    }

    /// Function checks the `election_ts` in current era is `finalized` and if it true returns it.
    ///
    /// By `finalization` we mean the 64 blocks, because of it is closely to real finalization time and
    /// reorgs for 64 blocks can not happen.
    fn election_timestamp_finalized(&self, chain_head: BlockHeader) -> Option<u64> {
        let timelines = self.db.protocol_timelines()?;

        let election_ts = timelines.era_end_ts(chain_head.timestamp) - timelines.election;

        (chain_head.timestamp.saturating_sub(election_ts)
            > alloy::eips::merge::SLOT_DURATION_SECS * 64)
            .then_some(election_ts)
    }
}
