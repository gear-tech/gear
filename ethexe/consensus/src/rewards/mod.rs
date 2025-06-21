use alloy::primitives::U256;
use ethexe_common::gear::{
    OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment,
};
use ethexe_db::Database;

struct RewardsWeights {
    block_producer: U256,
    block_validator: U256,
}

/// [`RewardsManager`] is responsible for managing rewards commitments.
/// It calculates the commiment only once per era and save it to the database.

#[derive(Debug, Clone)]
pub(crate) struct RewardsManager {
    db: Database,
}

impl RewardsManager {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Retuned vector can contains zero of one commitment.
    /// No commitment means that rewards are already distributed.
    pub fn create_commitment(&self) -> Result<Vec<RewardsCommitment>> {
        // TODO: check if rewards already distributed
        let rewards_required = true;
        if !rewards_required {
            Ok(vec![])
        }

        let rewards_commitment = RewardsCommitment {
            operators: self.operator_rewards_commitment(),
            stakers: self.stakers_rewards_commitment(),
            // TODO: add era timestamp
            timestamp: 0u64,
        };

        Ok(vec![rewards_commitment])
    }

    fn operator_rewards_commitment(&self) -> Result<OperatorRewardsCommitment> {
        let operators_rewards = OperatorRewardsCommitment {
            amount: 0,
            root: H256::zero(),
        };
        Ok(operator_rewards)
    }

    fn stakers_rewards_commitment(&self) -> Result<StakerRewardsCommitment> {
        let stakers_rewards = StakerRewardsCommitment {
            distribution: Vec::new(),
            total_amount: 0,
            token: H256::zero(),
        };
        Ok(stakers_rewards)
    }
}
