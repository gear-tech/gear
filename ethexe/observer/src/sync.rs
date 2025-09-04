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
use alloy::{
    consensus::BlockHeader as _,
    network::{BlockResponse, primitives::HeaderResponse},
    providers::{Provider, RootProvider},
    rpc::types::eth::{Block, Header},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    self, Address, BlockData, BlockHeader, CodeBlobInfo, OperatorStakingInfo, RewardsState,
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    events::{BlockEvent, RouterEvent},
    gear_core::pages::num_traits::Zero,
};
use ethexe_ethereum::{
    middleware::MiddlewareQuery, primitives::private::derive_more, router::RouterQuery,
};
use gprimitives::{H256, U256};
use nonempty::NonEmpty;
use std::collections::{BTreeMap, HashMap};

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
        let chain = self.load_chain(block, &header, blocks_data).await?;

        self.mark_chain_as_synced(chain.into_iter().rev());
        self.propagate_onchain_data(block, &header).await?;

        Ok(block)
    }

    async fn load_chain(
        &self,
        block: H256,
        header: &BlockHeader,
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

    // Propagate validators from the parent block. If start new era, fetch new validators from the router.
    async fn propagate_onchain_data(&self, block: H256, header: &BlockHeader) -> Result<()> {
        let era_first_block = self.era_first_block(&header)?;

        // propagate validators
        let validators = match self.db.validators(header.parent_hash) {
            Some(validators) if !era_first_block => validators,
            _ => {
                let fetched_validators = RouterQuery::from_provider(
                    self.config.router_address.0.into(),
                    self.provider.clone(),
                )
                .validators_at(block)
                .await?;

                NonEmpty::from_vec(fetched_validators).ok_or(anyhow!(
                    "validator set is empty on router for block({block})"
                ))?
            }
        };
        self.db.set_validators(block, validators.clone());

        // propagate information about rewarded era
        let rewards_state = match self.db.rewards_state(header.parent_hash) {
            Some(state) => state,
            None => {
                // fetch from router
                let latest_era = 0;
                RewardsState::LatestDistributed(latest_era)
            }
        };
        self.db.set_rewards_state(block, rewards_state);

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
    fn era_first_block(&self, chain_head: &BlockHeader) -> Result<bool> {
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

// THOUGHTS: create another struct to split the logic of onchain data

/// [`FinalizedDataSync`] works with finalized blocks and sync the necessary data.
#[derive(Clone, derive_more::Debug)]
pub(crate) struct FinalizedDataSync<DB: Clone> {
    #[debug(skip)]
    pub db: DB,
    pub provider: RootProvider,
    pub config: RuntimeConfig,
}

impl<DB: SyncDB> FinalizedDataSync<DB> {
    /// Entry point to process finalized block.
    pub async fn process_finalized_block(self, finalized_block: Block) -> Result<H256> {
        if self.can_load_staking_data(&finalized_block) {
            let _: () = self.sync_staking_data(&finalized_block).await?;
        }

        let finalized_block_hash = finalized_block.header().hash().0.into();
        self.db
            .set_latest_synced_finalized_block(finalized_block_hash);
        Ok(finalized_block_hash)
    }

    // Check if we can load staking data for the given finalized block.
    // We can load staking data if received block in a new era and not in the genesis.
    pub fn can_load_staking_data(&self, block: &Block) -> bool {
        let parent_hash = block.header().parent_hash().0.into();
        let parent = self
            .db
            .block_header(parent_hash)
            .expect("Expect parent header for finalized block exists in db");

        let era =
            (block.header().timestamp() - self.config.genesis_timestamp) / self.config.era_duration;
        let parent_era =
            (parent.timestamp - self.config.genesis_timestamp) / self.config.era_duration;

        era > 0 && era > parent_era
    }

    // Sync staking data for the given finalized block.
    async fn sync_staking_data(&self, block: &Block) -> Result<()> {
        let middleware_query = MiddlewareQuery::from_provider(
            self.config.middleware_address.0.into(),
            self.provider.root().clone(),
        );

        let validators = self
            .db
            .validators(block.header().hash().0.into())
            .expect("Must be propagate in `propagate_validators`");

        let mut operators_info = BTreeMap::new();
        for validator in validators.iter() {
            // THINK: maybe timestamp not from header
            let validator_stake = middleware_query
                .operator_stake_at(validator.0.into(), block.header().timestamp())
                .await?;

            let validator_stake_vaults = middleware_query
                .operator_stake_vaults_at(validator.0.into(), block.header().timestamp())
                .await?
                .iter()
                .map(|vault_with_stake| {
                    (
                        Address(vault_with_stake.vault.into_array()),
                        U256::from_little_endian(vault_with_stake.stake.as_le_slice()),
                    )
                })
                .collect();

            operators_info.insert(
                *validator,
                OperatorStakingInfo {
                    stake: U256(validator_stake.into_limbs()),
                    staked_vaults: validator_stake_vaults,
                },
            );
        }

        let era =
            (block.header().timestamp() - self.config.genesis_timestamp) / self.config.era_duration;

        self.db.mutate_staking_metadata(era, |metadata| {
            metadata.operators_info = operators_info;
        });

        Ok(())
    }
}
