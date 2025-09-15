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
    utils::{load_block_data, load_blocks_data_batched},
};
use alloy::{providers::RootProvider, rpc::types::eth::Header};
use anyhow::{Result, anyhow};
use ethexe_common::{
    BlockData, BlockHeader, CodeBlobInfo, GearExeTimelines, NextEraValidators, ValidatorsInfo,
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    end_of_era_timestamp, era_from_ts,
    events::{BlockEvent, RouterEvent},
};
use ethexe_ethereum::{middleware::MiddlewareQuery, router::RouterQuery};
use gprimitives::H256;
use std::collections::HashMap;

pub(crate) trait SyncDB:
    OnChainStorageRead + OnChainStorageWrite + BlockMetaStorageRead + BlockMetaStorageWrite + Clone
{
}
impl<
    T: OnChainStorageRead + OnChainStorageWrite + BlockMetaStorageRead + BlockMetaStorageWrite + Clone,
> SyncDB for T
{
}

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

        self.mark_chain_as_synced(chain.into_iter().rev());
        self.propagate_validators_info(block, &header).await?;

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
        while !self.db.block_meta(hash).synced {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    load_block_data(
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
        let Some(latest_synced_block_height) = self.db.latest_synced_block_height() else {
            tracing::warn!("latest_synced_block_height is not set in the database");
            return Ok(Default::default());
        };

        if header.height <= latest_synced_block_height {
            tracing::warn!(
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

    /// Propagate validators info. If block in the election period - make election if it not done.
    /// If block in the next era from parent, then make `next` era validators current.
    async fn propagate_validators_info(&self, block: H256, header: &BlockHeader) -> Result<()> {
        let header = self.db.block_header(header.parent_hash).ok_or(anyhow!(
            "header not found for parent block({:?})",
            header.parent_hash
        ))?;
        let timelines = self
            .db
            .gear_exe_timelines()
            .ok_or(anyhow!("not fonud gear exe timelines in db"))?;

        let mut validators_info = match self.db.validators_info(header.parent_hash) {
            Some(validators_info) => validators_info,
            None => {
                tracing::trace!(
                    "No validators info for parent block({:?}), query from router",
                    header.parent_hash
                );

                let router_address =
                    alloy::primitives::Address(self.config.router_address.0.into());
                let router_query =
                    RouterQuery::from_provider(router_address, self.provider.clone());
                let validators = router_query.validators_at(header.parent_hash).await?;
                let validators_info = ValidatorsInfo {
                    current: validators,
                    next: Default::default(),
                };
                self.db
                    .set_validators_info(header.parent_hash, validators_info.clone());
                validators_info
            }
        };

        // If next validators are already set, no need to fetch them again, because of
        // propagation in chain
        let era_election_ts = self.era_election_ts(header.timestamp, timelines);
        if validators_info.next == NextEraValidators::Unknown && header.timestamp >= era_election_ts
        {
            let middleware_query =
                MiddlewareQuery::new(self.provider.clone(), self.config.middleware_address);
            let elected_validators = middleware_query
                .make_election_at(era_election_ts, 10)
                .await?;
            validators_info.next = NextEraValidators::Elected(elected_validators);
        }

        // Switch validators from `next` to `current`
        if self.chain_head_in_next_era(&header, timelines)? {
            // Must be committed - handle then no.
            match validators_info.next.clone() {
                // Do nothing. Just propagate current validators.
                NextEraValidators::Unknown => {}

                // Remove the election state
                NextEraValidators::Elected(..) => {
                    validators_info.next = NextEraValidators::Unknown;
                }
                // Switch `next_validators` to current
                NextEraValidators::Committed(next_validators) => {
                    validators_info.next = NextEraValidators::Unknown;
                    validators_info.current = next_validators;
                }
            }
        }

        self.db.set_validators_info(block, validators_info);
        Ok(())
    }

    fn mark_chain_as_synced(&self, chain: impl Iterator<Item = H256>) {
        for hash in chain {
            let block_header = self
                .db
                .block_header(hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            self.db.mutate_block_meta(hash, |meta| meta.synced = true);

            self.db.set_latest_synced_block_height(block_header.height);
        }
    }

    /// NOTE: we don't need to fetch validators for block from zero era, because of
    /// it will be fetched in [`crate::ObserverService::pre_process_genesis_for_db`]
    fn chain_head_in_next_era(
        &self,
        chain_head: &BlockHeader,
        timelines: GearExeTimelines,
    ) -> Result<bool> {
        let parent = self.db.block_header(chain_head.parent_hash).ok_or(anyhow!(
            "header not found for block({:?})",
            chain_head.parent_hash
        ))?;

        let chain_head_era = era_from_ts(chain_head.timestamp, timelines.genesis_ts, timelines.era);
        let parent_era_index = era_from_ts(parent.timestamp, timelines.genesis_ts, timelines.era);

        Ok(chain_head_era > parent_era_index)
    }

    /// Returns the timestamp at which the current election started.
    fn era_election_ts(&self, block_ts: u64, timelines: GearExeTimelines) -> u64 {
        let block_era = era_from_ts(block_ts, timelines.genesis_ts, timelines.era);
        let end_of_block_era = end_of_era_timestamp(block_era, timelines.genesis_ts, timelines.era);
        end_of_block_era - timelines.election
    }
}
