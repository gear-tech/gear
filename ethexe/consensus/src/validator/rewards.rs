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

use std::collections::BTreeMap;

use anyhow::Result;
use ethexe_common::{
    Address, ProtocolTimelines,
    db::OnChainStorageRO,
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
};
use gprimitives::{H256, U256};

/// Protocol constant which defines the percent which will be distributed between operator stakers.
///
/// See [`OperatorWithEraTotalRewards::share_with_stakers`].
const STAKERS_REWARD_PERCENT: U256 = U256([90, 0, 0, 0]);
const PERCENT_DENOMINATOR: U256 = U256([100, 0, 0, 0]);

#[derive(Clone, derive_more::Debug)]
pub struct RewardsManager<DB: Clone> {
    #[debug(skip)]
    db: DB,

    timelines: ProtocolTimelines,
    wvara: Address,
}

impl<DB: OnChainStorageRO + Clone> RewardsManager<DB> {
    pub fn new(db: DB, timelines: ProtocolTimelines, wvara: Address) -> Self {
        Self {
            db,
            timelines,
            wvara,
        }
    }

    pub fn commitment_for(&self, era: u64) -> Result<RewardsCommitment> {
        let total_rewards = self.calculate_operators_era_total_rewards(era)?;

        let (operators, stakers) = total_rewards
            .into_iter()
            .map(|operator_total_rewards| {
                operator_total_rewards.share_with_stakers(self.db.clone())
            })
            .unzip();

        Ok(RewardsCommitment {
            operators: self.operators_commitment(operators),
            stakers: self.stakers_commitment(stakers),
            timestamp: self.timelines.era_end(era),
        })
    }

    /// Calculates the total rewards for every operator.
    /// Returns: vector of [`OperatorWithEraTotalRewards`]
    ///
    /// TODO (kuzmindev): should be improved by collecting the weighted metrics (in tokens) for each operator.
    fn calculate_operators_era_total_rewards(
        &self,
        era: u64,
    ) -> Result<Vec<OperatorWithEraTotalRewards>> {
        // let validators = self
        //     .db
        //     .validators(era)
        //     .ok_or_else(|| anyhow!("validators not found for era: {era:?}"))?;
        let validators = Vec::new();
        let mut operators_with_rewards = Vec::new();

        for validator in validators.into_iter() {
            let metrics = self.collect_operator_era_metrics(era, validator)?;
            operators_with_rewards.push(OperatorWithEraTotalRewards(
                validator,
                metrics.into_rewards(10),
            ));
        }
        Ok(operators_with_rewards)
    }

    fn collect_operator_era_metrics(
        &self,
        _era: u64,
        _operator: Address,
    ) -> Result<OperatorMetrics> {
        Ok(OperatorMetrics::default())
    }

    fn operators_commitment(
        &self,
        operators: Vec<OperatorWithRewards>,
    ) -> OperatorRewardsCommitment {
        let mut amount = U256::zero();
        operators.iter().for_each(|operator_rewards| {
            amount = amount.saturating_add(operator_rewards.1);
        });

        OperatorRewardsCommitment {
            amount,
            root: self.build_operator_merkle_root(operators),
        }
    }

    fn stakers_commitment(
        &self,
        stakers_rewards: Vec<StakersWithRewards>,
    ) -> StakerRewardsCommitment {
        let mut stakers_data = BTreeMap::<Address, U256>::new();
        let mut total_amount = U256::zero();

        stakers_rewards.into_iter().for_each(|stakers| {
            stakers.0.into_iter().for_each(|(vault, value)| {
                total_amount = total_amount.saturating_add(value);

                stakers_data
                    .entry(vault)
                    .and_modify(|v| {
                        *v = v.saturating_add(value);
                    })
                    .or_insert(value);
            });
        });

        StakerRewardsCommitment {
            distribution: stakers_data
                .into_iter()
                .map(|(vault, amount)| StakerRewards { vault, amount })
                .collect(),
            total_amount,
            token: self.wvara,
        }
    }

    fn build_operator_merkle_root(&self, mut operators: Vec<OperatorWithRewards>) -> H256 {
        // To guarantee the determenistic we need to sort operators by its address.
        operators.sort_by(|f, s| f.0.cmp(&s.0));
        let merkle_tree_data = operators
            .into_iter()
            .map(|operator| (operator.0.0.into(), operator.1))
            .collect();

        let tree = oz_merkle_rs::MerkleTree::new(merkle_tree_data);
        tree.get_root().expect("expect non empty operators rewards")
    }
}

/// Represents the operator with all rewards for actions for appropriate era.
/// The total rewards will be shared with stakers. So it is not the
#[derive(Debug, Clone)]
struct OperatorWithEraTotalRewards(Address, U256);

#[derive(Debug, Clone)]
struct OperatorWithRewards(Address, U256);

#[derive(Debug, Clone)]
struct StakersWithRewards(Vec<(Address, U256)>);

impl OperatorWithEraTotalRewards {
    fn share_with_stakers<DB: OnChainStorageRO>(
        self,
        _db: DB,
    ) -> (OperatorWithRewards, StakersWithRewards) {
        let total_stakers_rewards = self
            .1
            .saturating_mul(PERCENT_DENOMINATOR)
            .checked_div(STAKERS_REWARD_PERCENT)
            .expect("STAKERS_REWARDS_PERCENT can not be equal 0");
        let operator_rewards = self.1.saturating_sub(total_stakers_rewards);

        // TODO (kuzmindev): should be improved by querying from database and and fetching it to observer.
        let validator_stakers: Vec<(Address, U256)> = Vec::new();
        let total_operator_stake = U256::zero();

        let staker_rewards = validator_stakers
            .into_iter()
            .map(|(vault_address, stake)| {
                let vault_rewards = stake
                    .saturating_mul(total_stakers_rewards)
                    .checked_div(total_operator_stake)
                    .expect("total stakers_rewards couldn't be zero");
                (vault_address, vault_rewards)
            })
            .collect();
        (
            OperatorWithRewards(self.0, operator_rewards),
            StakersWithRewards(staker_rewards),
        )
    }
}

/// [`OperatorMetrics`] represents the statistics for operator's actions for which
/// rewards should be distribute.
///
/// Metrics represents in [`u32`], because of now in ethereum we have `7 200` blocks per era (1 day).
/// So [`u32::max`] divided by `7 200` equals: `596 523`. We assume that this amount of actions is a great enough.
#[derive(Clone, Debug, Default)]
struct OperatorMetrics {
    /// The number of [`ethexe_common::gear::BatchCommitment`] which operator submits to Ethereum.
    batches_commited: u32,
    /// The number of blocks which this operator validated.
    blocks_validated: u32,
    /// The number of valid promises which validator gives to users.
    valid_promises_given: u32,
}

/// Module defines the amount of WVara tokens for operator's actions.
/// Weights represents in WVara tokens. To calculate the resulting tokens value must be multiply by WVara token digets.
mod weights {
    use super::U256;

    pub const BATCH_COMMITED: U256 = U256([1000, 0, 0, 0]);
    pub const BLOCK_VALIDATED: U256 = U256([100, 0, 0, 0]);
    pub const VALID_PROMISE: U256 = U256([300, 0, 0, 0]);
}

impl OperatorMetrics {
    /// Converts the [`OperatorMetrics`] into the rewards value in tokens.
    ///
    /// TODO (kuzmindev): should be improved by iterating the blocks in era and collecting the
    /// appropriate data.
    fn into_rewards(self, wvara_digets: u8) -> U256 {
        let mut rewards = U256::from(self.batches_commited)
            .saturating_mul(weights::BATCH_COMMITED.pow(U256::from(wvara_digets)));
        rewards = rewards.saturating_add(
            U256::from(self.blocks_validated)
                .saturating_mul(weights::BLOCK_VALIDATED.pow(U256::from(wvara_digets))),
        );
        rewards = rewards.saturating_add(
            U256::from(self.valid_promises_given)
                .saturating_mul(weights::VALID_PROMISE.pow(U256::from(wvara_digets))),
        );
        rewards
    }
}
