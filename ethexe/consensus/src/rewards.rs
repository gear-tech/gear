use std::collections::BTreeMap;

use alloy::{
    eips::{BlockId, HashOrNumber},
    primitives::Address,
    providers::{Provider, RootProvider},
};
use ethexe_common::{
    db::{BlockMetaStorageRead, OnChainStorageRead},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
    BlockHeader,
};
use gprimitives::{H256, U256};

use ethexe_db::Database;
use futures::StreamExt;

/// Weights for rewards calculation.
/// Weights calculates in `wVARA` tokens as `amount` * 10 ** `vVARA decimals`.
mod weights {
    // Reward for operator participation in the commitment.
    pub const PARTICIPATION: u64 = 1000;

    // Reward for operator commit new execution state to Ethereum.
    // Total rewards for commitment will be calculated as:
    // `VALIDATOR_COMMITMENT_PER_GAS * gas_used
    pub const COMMITMENT_GAS_UNIT: u64 = 1;
}

// TODO: wait for 3 eth eras to calculate rewards
/*
* Rewards proporsal*
1. watch finalized blocks (starting from 2 eras ago)
2. iterate through all finalized blocks and
   - collect all block producers and validators
   - collect all stakers
3. calculate vaults staking rewards at the beginning of the election era
4.
 - producer: propose rewards commitment if its not already proposed
 - participant: check the correctness of the rewards commitment


NOTES:
- create the criteria for need to send rewards commitment
- key for blockHash `era:blockHash` - can use rocksdb method for iterate over all prefix keys `era:...`
- consensus doesn't load anything from the eth rpc

Validators count in db:
- 0: iter through all previous eras and find latest setted validators
- 1: best case
- 2: iter through all parent blocks and find one of the blocks from parent blocks
*/

#[derive(Debug, Clone)]
pub struct RewardsManagerConfig {
    pub genesis_timestamp: u64,
    pub era_duration: u64,
    pub wvara_digets: U256,
}

#[derive(thiserror::Error, Debug)]
pub enum DistributionError {
    #[error("previous not empty block not found for {0:?}")]
    PreviousBlockNotFound(H256),
    #[error("block header not found for: {0:?}")]
    BlockHeaderNotFound(H256),
}

type Result<T> = std::result::Result<T, DistributionError>;

/// [`RewardsManager`] is responsible for managing rewards commitments.
/// It calculates the commiment only once per era and save it to the database.
#[derive(Clone)]
pub(crate) struct RewardsManager<DB: BlockMetaStorageRead + OnChainStorageRead> {
    db: DB,
    config: RewardsManagerConfig,
}

impl<DB: BlockMetaStorageRead + OnChainStorageRead> RewardsManager<DB> {
    pub fn new(db: DB, config: RewardsManagerConfig) -> Self {
        Self { db, config }
    }

    /// Retuned vector can contains zero or one commitment.
    /// No commitment means that rewards are already distributed.
    pub fn create_commitment(&self, chain_head: H256) -> Result<Option<RewardsCommitment>> {
        if !self.distribution_criteria(chain_head)? {
            return Ok(None);
        }

        let rewards_commitment = RewardsCommitment {
            operators: self.operator_rewards_commitment()?,
            stakers: self.stakers_rewards_commitment()?,
            // TODO: add era timestamp
            timestamp: 0u64,
        };

        Ok(Some(rewards_commitment))
    }

    // Criteria for distribution of rewards:
    // 1. In the current era rewards are not distributed yet
    // 2. Era is finished (current era is not equal to latest rewarded era + 1)
    fn distribution_criteria(&self, chain_head: H256) -> Result<bool> {
        let header = self
            .db
            .block_header(chain_head)
            .ok_or(DistributionError::BlockHeaderNotFound(chain_head))?;
        let parent = self
            .db
            .previous_not_empty_block(chain_head)
            .ok_or(DistributionError::PreviousBlockNotFound(chain_head))?;

        // Check rewards are already distributed
        let latest_rewarded_era = self
            .db
            .latest_rewarded_era(chain_head)
            .unwrap_or(self.era_index(header.timestamp));
        let current_era =
            (header.timestamp - self.config.genesis_timestamp) / self.config.era_duration;

        if current_era == latest_rewarded_era {
            // rewards can not be distribute, because of in this era they were already
            return Ok(false);
        }

        if current_era == latest_rewarded_era + 1 {
            // rewards can't be distributed, because era is not finished yet
            return Ok(false);
        }

        // maybe need check something else
        Ok(true)
    }

    fn era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.config.genesis_timestamp) / self.config.era_duration
    }

    fn operator_rewards_commitment(&self, block_hash: H256) -> Result<OperatorRewardsCommitment> {
        let (operators_rewards_data, total_rewards) = self.collect_operators_rewards(block_hash)?;
        let root = utils::build_merkle_tree(operators_rewards_data);

        let operator_rewards = OperatorRewardsCommitment {
            amount: total_rewards,
            root,
        };
        Ok(operator_rewards)
    }

    fn collect_operator_rewards(
        &self,
        chain_head: BlockHeader,
    ) -> Result<(BTreeMap<Address, U256>, U256)> {
        let distribution_era = self.era_index(chain_head.timestamp);
        let mut current_block = block;

        let mut operators_rewards = BTreeMap::new();
        let mut total_rewards = U256::zero();
        loop {
            let block_header = self
                .db
                .block_header(current_block)
                .ok_or(DistributionError::BlockHeaderNotFound(current_block))?;
            let block_era = self.era_index(block_header.timestamp);

            if block_era < distribution_era {
                // We are in the past, no need to continue
                break;
            }

            if block_era > distribution_era {
                // We are in the future, skip blocks not from the distribution era
                current_block = block_header.parent_hash;
                continue;
            }

            let block_validators = self.db
        }

        Ok((operators_rewards, total_rewards))
    }

    fn stakers_rewards_commitment(&self) -> Result<StakerRewardsCommitment> {
        let stakers_rewards = StakerRewardsCommitment {
            distribution: Vec::new(),
            total_amount: U256::zero(),
            token: Address::ZERO.into_array().into(),
        };
        Ok(stakers_rewards)
    }
}

mod utils {
    use super::*;

    fn build_merkle_tree(rewards_data: BTreeMap<Address, U256>) -> H256 {
        todo!()
    }
}

#[cfg(test)]
mod tests {}
