use ethexe_common::{
    Address, ToDigest,
    db::{BlockMetaStorageRead, OnChainStorageRead, RewardsState},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
};
use gprimitives::{H160, H256, U256};
use sha3::Digest;
use std::{collections::BTreeMap, ops::Range, time::Duration};

#[cfg(test)]
mod tests;

/*
TODO: wait for 3 eth eras to calculate rewards
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
- 0: iter through all previous eras and find latest set validators
- 1: best case
- 2: iter through all parent blocks and find one of the blocks from parent blocks
*/

const STAKER_REWARDS_RATIO: u32 = 90; // 90% of rewards goes to stakers

const REWARDS_CONFIRMATION_BLOCKS_WINDOW: u64 = 5;

#[derive(thiserror::Error, Debug)]
pub enum RewardsError {
    #[error("block header not found for: {0:?}")]
    BlockHeader(H256),
    #[error("validators not found for block({0:?})")]
    BlockValidators(H256),
    #[error("operator stake vaults not found for block({0:?}")]
    OperatorStakeVaults(H160),
    #[error("stake not found for operator({0:?}) in era {1}")]
    OperatorEraStake(H160, u64),
    #[error("...")]
    RewardsDistribution(u64),
    #[error("...")]
    RewardsStateNotFound(H256),
}
type Result<T> = std::result::Result<T, RewardsError>;

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone)]
pub(crate) struct RewardsConfig {
    pub genesis_timestamp: u64,
    pub era_duration: u64,
    pub slot_duration: Duration,
    pub wvara_digests: U256,
    pub wvara_address: Address,
}

#[derive(Clone, derive_more::Debug)]
pub(crate) struct RewardsManager<DB>
where
    DB: OnChainStorageRead + BlockMetaStorageRead,
{
    #[debug(skip)]
    db: DB,
    config: RewardsConfig,
}

impl<DB: OnChainStorageRead + BlockMetaStorageRead> RewardsManager<DB> {
    pub fn new(db: DB, config: RewardsConfig) -> Self {
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

        // Cumulative rewards are initialize by default with
        let mut cumulative_operator_rewards = self
            .db
            .operators_rewards_distribution_at(0)
            .ok_or(RewardsError::RewardsDistribution(0))?;
        let mut total_operator_rewards = U256::zero();

        let mut cumulative_vault_rewards = BTreeMap::new();
        let mut total_staker_rewards = U256::zero();

        for era in eras_to_reward {
            let (mut operators_rewards, era_total_amount) =
                self.collect_era_rewards(era, block_hash)?;

            let vault_rewards = self.extract_vault_rewards(era, &mut operators_rewards)?;

            total_operator_rewards += era_total_amount;
            operators_rewards.into_iter().for_each(|(address, amount)| {
                cumulative_operator_rewards
                    .entry(address)
                    .and_modify(|e| *e += amount)
                    .or_insert(amount);
            });

            vault_rewards.into_iter().for_each(|(address, amount)| {
                total_staker_rewards += amount;

                cumulative_vault_rewards
                    .entry(address)
                    .and_modify(|e| *e += amount)
                    .or_insert(amount);
            });
        }

        let operators_commitment = OperatorRewardsCommitment {
            amount: total_operator_rewards,
            root: utils::operators_merkle_tree(cumulative_operator_rewards),
        };

        let stakers_commitment = StakerRewardsCommitment {
            distribution: cumulative_vault_rewards
                .into_iter()
                .map(|(vault, amount)| StakerRewards { vault, amount })
                .collect(),
            total_amount: total_staker_rewards,
            token: self.config.wvara_address,
        };

        Ok(Some(RewardsCommitment {
            operators: operators_commitment,
            stakers: stakers_commitment,
            timestamp: header.timestamp,
        }))
    }

    fn eras_to_reward(&self, block_hash: H256, block_timestamp: u64) -> Result<Option<Range<u64>>> {
        // THINK: maybe need to fetch from router, not use default value - 0
        // let latest_rewarded_era = db.latest_rewarded_era(block_hash).unwrap_or_default();
        let latest_rewarded_era = match self
            .db
            .rewards_state(block_hash)
            .ok_or(RewardsError::RewardsStateNotFound(block_hash))?
        {
            RewardsState::LatestDistributed(era) => era,
            RewardsState::SentToEthereum {
                in_block,
                previous_rewarded,
            } => {
                if self.should_wait_for_rewards_confirmation_in(in_block, block_timestamp)? {
                    return Ok(None);
                }

                previous_rewarded
            }
        };

        let current_era = utils::era_index(&self.config, block_timestamp);

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

    fn should_wait_for_rewards_confirmation_in(
        &self,
        block: H256,
        current_block_timestamp: u64,
    ) -> Result<bool> {
        let header = self
            .db
            .block_header(block)
            .ok_or(RewardsError::BlockHeader(block))?;

        let blocks_came =
            (current_block_timestamp - header.timestamp) / self.config.slot_duration.as_secs();

        Ok(blocks_came < REWARDS_CONFIRMATION_BLOCKS_WINDOW)
    }

    fn collect_era_rewards(
        &self,
        era: u64,
        chain_head: H256,
    ) -> Result<(BTreeMap<Address, U256>, U256)> {
        let mut current_block = chain_head;
        let mut rewards_statistics = BTreeMap::new();
        let mut total_rewards = U256::zero();

        loop {
            let block_header = self
                .db
                .block_header(current_block)
                .ok_or(RewardsError::BlockHeader(current_block))?;
            let block_era = utils::era_index(&self.config, block_header.timestamp);

            if era <= block_era {
                // We are in the future, no need to continue
                continue;
            }

            if era > block_era {
                // We are in the past, no need to continue
                break;
            }

            let block_validators = self
                .db
                .validators(current_block)
                .ok_or(RewardsError::BlockValidators(current_block))?;

            for validator in block_validators.iter() {
                let operator_rewards = rewards_statistics.entry(*validator).or_insert(U256::zero());

                let value = U256::from(100) * U256::from(10).pow(self.config.wvara_digests);

                *operator_rewards += value;
                total_rewards += value;
            }
            current_block = block_header.parent_hash;
        }

        Ok((rewards_statistics, total_rewards))
    }

    /// Split rewards on validators rewards and stakers rewards
    fn extract_vault_rewards(
        &self,
        era: u64,
        operators_rewards: &mut BTreeMap<Address, U256>,
    ) -> Result<BTreeMap<Address, U256>> {
        let mut vault_rewards = BTreeMap::new();
        for (address, amount) in operators_rewards.iter_mut() {
            let staker_amount = *amount * U256::from(STAKER_REWARDS_RATIO) / U256::from(100);
            *amount -= staker_amount;

            let operator_total_stake = self
                .db
                .operator_stake_at(H160(address.0), era)
                .ok_or(RewardsError::OperatorEraStake(H160(address.0), era))?;

            let stake_vaults = self
                .db
                .operator_stake_vaults_at(H160(address.0), era)
                .ok_or(RewardsError::OperatorStakeVaults(H160(address.0)))?;

            for (vault, stake_in_vault) in stake_vaults {
                let vault_rewards = vault_rewards.entry(vault).or_insert(U256::zero());
                *vault_rewards += (staker_amount * stake_in_vault) / operator_total_stake;
            }
        }
        Ok(vault_rewards)
    }
}

mod utils {
    use super::*;

    pub fn era_index(config: &RewardsConfig, block_ts: u64) -> u64 {
        (block_ts - config.genesis_timestamp) / config.era_duration
    }

    pub fn operators_merkle_tree(rewards: BTreeMap<Address, U256>) -> H256 {
        let leaves = rewards
            .into_iter()
            // Maybe need to sort vec here, because of the position of elements may be different
            .map(|(address, amount)| {
                let mut hasher = sha3::Keccak256::new();
                hasher.update(address.0);
                hasher.update(<[u8; 32]>::from(amount));
                hasher.finalize().to_digest().0
            })
            .collect::<Vec<_>>();

        let tree = rs_merkle::MerkleTree::<rs_merkle::algorithms::Keccak256>::from_leaves(
            leaves.as_slice(),
        );

        // Tree is nonempty, because of validator set is nonempty and at least one operator has rewards
        tree.root()
            .expect("Nonempty merkle tree should have a root")
            .into()
    }
}
