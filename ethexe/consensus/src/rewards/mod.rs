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

#![allow(clippy::all)]

use alloy::providers::Provider;
use ethexe_common::{
    Address, RewardsState,
    db::{BlockMetaStorageRead, OnChainStorageRead},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
};
use ethexe_ethereum::{router::RouterQuery, wvara::WVaraQuery};
use gprimitives::{H160, H256, U256};
use oz_merkle_rs::MerkleTree;
use std::{collections::BTreeMap, ops::Range};

#[cfg(test)]
mod tests;

/// Constants for rewards weights in network activity.
mod weights {
    use super::U256;

    /// The weight for a validated block in base units.
    pub const VALIDATED_BLOCK: U256 = U256([100u64, 0, 0, 0]); // 100 base units
}

// Number of blocks to wait for commitment confirmation in Ethereum
const REWARDS_CONFIRMATION_SLOTS_WINDOW: u64 = 5;
const STAKER_REWARDS_RATIO: u32 = 90;
const PERCENTAGE_DENOMINATOR: u32 = 100;

/// Window to wait for previous era
const PREV_ERA_: u64 = alloy::eips::merge::SLOT_DURATION * 10;

#[derive(thiserror::Error, Debug)]
pub enum RewardsError {
    #[error("block header not found for: {0:?}")]
    BlockHeader(H256),
    #[error("validators not found for block({0:?})")]
    BlockValidators(H256),
    // #[error("operator stake vaults not found for block({0:?}")]
    // OperatorStakeVaults(H160),
    // #[error("stake not found for operator({0:?}) in era {1}")]
    // OperatorEraStake(H160, u64),
    // #[error("rewards distribution not found for era {0}")]
    // RewardsDistribution(u64),
    #[error("rewards state not found for block {0}")]
    RewardsStateNotFound(H256),
    #[error(transparent)]
    Transport(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
    #[error("anyhow error: {0}")]
    Any(#[from] anyhow::Error),
}
type Result<T> = std::result::Result<T, RewardsError>;

#[derive(Clone, derive_more::Debug)]
struct TotalEraRewards<DB> {
    pub inner: BTreeMap<Address, U256>,
    pub era: u64,

    #[debug(skip)]
    db: DB,
}

impl<DB: OnChainStorageRead + BlockMetaStorageRead> TotalEraRewards<DB> {
    // Splits the total rewards into operator and staker rewards.
    pub fn split(self) -> Result<EraRewards> {
        let mut operators = self.inner;
        let mut stakers = BTreeMap::new();
        let mut total_operator_rewards = U256::zero();
        let mut total_staker_rewards = U256::zero();

        let staking_metadata = self.db.staking_metadata(self.era).unwrap();
        for (operator, amount) in operators.iter_mut() {
            let staker_amount =
                *amount * U256::from(STAKER_REWARDS_RATIO) / U256::from(PERCENTAGE_DENOMINATOR);
            *amount -= staker_amount;

            total_operator_rewards += *amount;
            total_staker_rewards += staker_amount;

            let operator_stake_vaults = staking_metadata
                .operators_staked_vaults
                .get(operator)
                .unwrap();
            let operator_total_stake = staking_metadata.operators_stake.get(operator).unwrap();

            for (vault, stake_in_vault) in operator_stake_vaults {
                let vault_rewards = stakers.entry(*vault).or_insert(U256::zero());
                *vault_rewards += (staker_amount * stake_in_vault) / *operator_total_stake;
            }
        }

        Ok(EraRewards {
            inner: RewardsDistribution { operators, stakers },
            era: self.era,
        })
    }
}

#[derive(Clone, Debug)]
struct Context {
    token: Address,
    timestamp: u64,
}

#[derive(Clone, Debug)]
struct RewardsDistribution {
    pub operators: BTreeMap<Address, U256>,
    pub stakers: BTreeMap<Address, U256>,
}

// Rewards distributions for particular era.
// This structure creates from `TotalEraRewards` by calling `split` method.
#[derive(Clone, Debug)]
struct EraRewards {
    pub inner: RewardsDistribution,
    #[allow(unused)]
    pub era: u64,
}

/// [`CumulativeRewards`] represents the cumulative rewards from  all processed eras.
#[derive(Clone, Debug)]
struct CumulativeRewards {
    inner: RewardsDistribution,

    // Stores for concrete era total rewards for (stakers, operators).
    pub total_operator_rewards: U256,
    pub total_staker_rewards: U256,
}

impl CumulativeRewards {
    pub fn initialize_from_db<DB: OnChainStorageRead>(db: DB, block_hash: H256) -> Result<Self> {
        // Inner field constructs from the previous rewarded era.
        let last_rewarded_era = match db
            .rewards_state(block_hash)
            .ok_or(RewardsError::RewardsStateNotFound(block_hash))?
        {
            RewardsState::LatestDistributed(era) => era,
            RewardsState::SentToEthereum {
                previous_rewarded_era,
                ..
            } => previous_rewarded_era,
        };

        let staking_metadata = db.staking_metadata(last_rewarded_era).unwrap();
        Ok(Self {
            inner: RewardsDistribution {
                operators: staking_metadata.operators_rewards_distribution,
                stakers: BTreeMap::new(),
            },
            total_operator_rewards: U256::zero(),
            total_staker_rewards: U256::zero(),
        })
    }

    pub fn into_commitment(self, ctx: Context) -> RewardsCommitment {
        let merkle_tree = utils::build_merkle_tree(self.inner.operators);
        let root = merkle_tree
            .get_root()
            .expect("Nonempty merkle tree should have a root");

        let operators_commitment = OperatorRewardsCommitment {
            amount: self.total_operator_rewards,
            root,
        };

        let stakers_commitment = StakerRewardsCommitment {
            distribution: self
                .inner
                .stakers
                .into_iter()
                .map(|(vault, amount)| StakerRewards { vault, amount })
                .collect(),
            total_amount: self.total_staker_rewards,
            token: ctx.token,
        };

        RewardsCommitment {
            operators: operators_commitment,
            stakers: stakers_commitment,
            timestamp: ctx.timestamp,
        }
    }

    pub fn extend(&mut self, other: EraRewards) {
        let EraRewards {
            inner:
                RewardsDistribution {
                    operators: other_operators,
                    stakers: other_stakers,
                },
            ..
        } = other;

        for (operator, amount) in other_operators {
            self.total_operator_rewards += amount;

            self.inner
                .operators
                .entry(operator)
                .and_modify(|a| *a += amount)
                .or_insert(amount);
        }

        for (vault, amount) in other_stakers {
            self.total_staker_rewards += amount;

            self.inner
                .stakers
                .entry(vault)
                .and_modify(|a| *a += amount)
                .or_insert(amount);
        }
    }
}

#[cfg_attr(test, derive(Default))]
#[derive(Clone, Debug)]
struct Config {
    pub genesis_timestamp: u64,
    pub era_duration: u64,
    pub slot_duration_secs: u64,
    pub wvara_decimals: u8,
    pub wvara_address: Address,
}

/// [`RewardsManager`] is responsible for managing rewards distribution in the Gear.exe network.
/// TODO: redesign rewards manager in future when Symbiotic release contracts for staker rewards
/// which will use merkle tree (same as for operator rewards).

#[derive(Clone, derive_more::Debug)]
pub(crate) struct RewardsManager<DB> {
    #[debug(skip)]
    db: DB,
    config: Config,
}

impl<DB: OnChainStorageRead + BlockMetaStorageRead + Clone> RewardsManager<DB> {
    pub async fn new(db: DB, router_query: RouterQuery) -> Result<Self> {
        let genesis_block_hash = router_query.genesis_block_hash().await?;
        let genesis_timestamp = router_query
            .provider()
            .get_block_by_hash(genesis_block_hash.0.into())
            .await?
            .unwrap()
            .header
            .timestamp;
        let era_duration = router_query.timelines().await?.era;

        let wvara_address = router_query.wvara_address().await?.0.0.into();
        let wvara_decimals = WVaraQuery::from_provider(wvara_address, router_query.provider())
            .decimals()
            .await?;

        let config = Config {
            genesis_timestamp,
            era_duration,
            slot_duration_secs: alloy::eips::merge::SLOT_DURATION_SECS,
            wvara_decimals,
            wvara_address,
        };

        Ok(Self { db, config })
    }

    #[cfg(test)]
    pub fn mock(db: DB) -> Self {
        let config = Default::default();
        Self { db, config }
    }

    pub fn create_commitment(&self, block_hash: H256) -> Result<Option<RewardsCommitment>> {
        let header = self
            .db
            .block_header(block_hash)
            .ok_or(RewardsError::BlockHeader(block_hash))?;

        let Some(eras_to_reward) = self.eras_to_reward(block_hash, header.timestamp)? else {
            return Ok(None);
        };

        let mut cumulative_rewards =
            CumulativeRewards::initialize_from_db(self.db.clone(), block_hash)?;

        for era in eras_to_reward {
            let total_era_rewards = self.era_total_rewards(era, block_hash)?;
            let rewards = total_era_rewards.split()?;
            cumulative_rewards.extend(rewards);
        }

        let context = Context {
            token: self.config.wvara_address,
            timestamp: header.timestamp,
        };
        Ok(Some(cumulative_rewards.into_commitment(context)))
    }

    // Returns the range of eras for which rewards can be distributed.
    fn eras_to_reward(&self, block_hash: H256, block_timestamp: u64) -> Result<Option<Range<u64>>> {
        let rewards_state = self
            .db
            .rewards_state(block_hash)
            .ok_or(RewardsError::RewardsStateNotFound(block_hash))?;

        let latest_rewarded_era = match rewards_state {
            RewardsState::LatestDistributed(era) => era,
            RewardsState::SentToEthereum {
                in_block,
                previous_rewarded_era,
                ..
            } => {
                if self.should_wait_for_rewards_confirmation(in_block, block_timestamp)? {
                    return Ok(None);
                }

                previous_rewarded_era
            }
        };

        let current_era = self.era_index(block_timestamp);

        if current_era == latest_rewarded_era {
            // rewards can not be distribute, because of in this era they were already
            return Ok(None);
        }

        if current_era == latest_rewarded_era + 1 {
            // rewards can't be distributed, because era is not finished yet
            return Ok(None);
        }

        // maybe need check something else
        Ok(Some(latest_rewarded_era..current_era))
    }

    // Returns whether we should wait for rewards confirmation in the next `REWARDS_CONFIRMATION_SLOTS_WINDOW` slots.
    // If true, it means that the rewards commitment is not yet confirmed and we should not create another commitment.
    fn should_wait_for_rewards_confirmation(
        &self,
        in_block: H256,
        current_block_ts: u64,
    ) -> Result<bool> {
        let header = self
            .db
            .block_header(in_block)
            .ok_or(RewardsError::BlockHeader(in_block))?;

        let slots_came = (current_block_ts - header.timestamp) / self.config.slot_duration_secs;
        Ok(slots_came < REWARDS_CONFIRMATION_SLOTS_WINDOW)
    }

    fn era_total_rewards(&self, era: u64, mut block: H256) -> Result<TotalEraRewards<DB>> {
        let mut rewards_statistics = BTreeMap::new();
        let mut total_rewards = U256::zero();

        loop {
            let block_header = self
                .db
                .block_header(block)
                .ok_or(RewardsError::BlockHeader(block))?;
            let block_era = self.era_index(block_header.timestamp);

            if era < block_era {
                block = block_header.parent_hash;
                // We are in the future, skip this block
                continue;
            }

            if era > block_era {
                // We are in the past, no need to continue
                break;
            }

            let block_validators = self
                .db
                .validators(block)
                .ok_or(RewardsError::BlockValidators(block))?;

            for validator in block_validators.iter() {
                let base_unit = U256::from(10u64.pow(self.config.wvara_decimals as u32));
                let value = weights::VALIDATED_BLOCK * base_unit;

                rewards_statistics
                    .entry(*validator)
                    .and_modify(|v| *v += value)
                    .or_insert(value);
                total_rewards += value;
            }

            block = block_header.parent_hash;
        }

        Ok(TotalEraRewards {
            inner: rewards_statistics,
            era,
            db: self.db.clone(),
        })
    }

    #[inline(always)]
    pub fn era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.config.genesis_timestamp) / self.config.era_duration
    }
}

mod utils {
    use super::*;

    pub fn build_merkle_tree(rewards: BTreeMap<Address, U256>) -> MerkleTree {
        let values = rewards
            .into_iter()
            .map(|(address, amount)| (H160(address.0), amount))
            .collect::<Vec<_>>();
        MerkleTree::new(values)
    }
}
