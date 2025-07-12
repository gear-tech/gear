use alloy::{
    eips::{BlockId, HashOrNumber},
    primitives::Address,
    providers::{Provider, RootProvider},
};
use anyhow::Result;
use ethexe_common::{
    db::{BlockMetaStorageRead, OnChainStorageRead},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
};
use gprimitives::{H256, U256};

use ethexe_db::Database;
use futures::StreamExt;

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

struct RewardsWeights {
    block_producer: U256,  // 10 VARA tokens by default
    block_validator: U256, // 10 VARA tokens by default
}

#[derive(thiserror::Error)]
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
pub(crate) struct RewardsManager {
    db: Database,
    config: RewardsManagerConfig,
}

impl RewardsManager {
    pub fn new(db: Database, config: RewardsManagerConfig) -> Self {
        Self { db, config }
    }

    /// Retuned vector can contains zero of one commitment.
    /// No commitment means that rewards are already distributed.
    pub fn create_commitment(&self, chain_head: H256) -> Result<Vec<RewardsCommitment>> {
        if !self.distribution_criteria(chain_head) {
            return Ok(vec![]);
        }

        // let block = self.provider.get_block_by_hash(hash).await?;

        // let mut stream = self.provider.subscribe_blocks().await?.into_stream();
        // while let Some(block) = stream.next().await {
        //     let block = block.inner;
        // }

        // let rewards_commitment = RewardsCommitment {
        //     operators: self.operator_rewards_commitment(),
        //     stakers: self.stakers_rewards_commitment(),
        //     // TODO: add era timestamp
        //     timestamp: 0u64,
        // };

        // Ok(vec![rewards_commitment])
        todo!()
    }

    // Criteria for distribution of rewards:
    // 1. all blocks from the era of distribution are finalized
    // 2. rewards are not already distributed
    fn distribution_criteria(&self, chain_head: H256) -> Result<bool> {
        let parent = self
            .db
            .previous_not_empty_block(chain_head)
            .ok_or(DistributionError::PreviousBlockNotFound(chain_head))?;

        // Check rewards are already distributed
        if let Some(latest_rewarded_era) = self.db.latest_rewarded_era(chain_head) {
            let header = self
                .db
                .block_header(chain_head)
                .ok_or(DistributionError::BlockHeaderNotFound(chain_head))?;
            let current_era =
                (header.timestamp - self.config.genesis_timestamp) / self.config.era_duration;

            if (current_era == latest_rewarded_era) {
                // rewards can not be distribute, because of in this era they were already
                return Ok(false);
            }
        };

        // TODO: check enough eras are came to distribute rewards
        todo!()
    }

    fn all_era_blocks_are_finalized(&self) -> Result<bool> {
        todo!()
    }

    fn operator_rewards_commitment(&self) -> Result<OperatorRewardsCommitment> {
        let operator_rewards = OperatorRewardsCommitment {
            amount: U256::zero(),
            root: H256::zero(),
        };
        Ok(operator_rewards)
    }

    fn stakers_rewards_commitment(&self) -> Result<StakerRewardsCommitment> {
        let stakers_rewards = StakerRewardsCommitment {
            distribution: Vec::new(),
            total_amount: U256::zero(),
            token: Address::ZERO.into_array(),
        };
        Ok(stakers_rewards)
    }
}

#[cfg(test)]
mod tests {}
