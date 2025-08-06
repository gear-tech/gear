use alloy::providers::Provider;
use ethexe_common::{
    Address,
    db::{BlockMetaStorageRead, OnChainStorageRead, RewardsState},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewards, StakerRewardsCommitment},
};
use ethexe_ethereum::router::RouterQuery;
use gprimitives::{H160, H256, U256};
use oz_merkle_rs::MerkleTree as OzMerkleTree;
use std::{collections::BTreeMap, ops::Range};

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
    #[error("rewards distribution not found for era {0}")]
    RewardsDistribution(u64),
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
    pub fn split_into_rewards(self) -> Result<Rewards> {
        let mut operators = self.inner;
        let mut stakers = BTreeMap::new();
        let total_operator_rewards = U256::zero();
        let total_staker_rewards = U256::zero();

        for (operator, amount) in operators.iter_mut() {
            let staker_amount = *amount * U256::from(STAKER_REWARDS_RATIO) / U256::from(100);
            *amount -= staker_amount;

            let operator_total_stake = self
                .db
                .operator_stake_at(H160(operator.0), self.era)
                .ok_or(RewardsError::OperatorEraStake(H160(operator.0), self.era))?;

            let stake_vaults = self
                .db
                .operator_stake_vaults_at(H160(operator.0), self.era)
                .ok_or(RewardsError::OperatorStakeVaults(H160(operator.0)))?;

            for (vault, stake_in_vault) in stake_vaults {
                let vault_rewards = stakers.entry(vault).or_insert(U256::zero());
                *vault_rewards += (staker_amount * stake_in_vault) / operator_total_stake;
            }
        }

        Ok(Rewards {
            operators,
            stakers,
            total_operator_rewards,
            total_staker_rewards,
        })
    }
}

#[derive(Clone, derive_more::Debug)]
struct Rewards {
    pub operators: BTreeMap<Address, U256>,
    pub stakers: BTreeMap<Address, U256>,
    pub total_operator_rewards: U256,
    pub total_staker_rewards: U256,
}

impl Rewards {
    pub fn into_commitment(self) -> RewardsCommitment {
        let merkle_tree = utils::build_merkle_tree(self.operators);
        let operators_commitment = OperatorRewardsCommitment {
            amount: self.total_operator_rewards,
            root: merkle_tree
                .get_root()
                .expect("Nonempty merkle tree should have a root"),
        };

        let stakers_commitment = StakerRewardsCommitment {
            distribution: self
                .stakers
                .into_iter()
                .map(|(vault, amount)| StakerRewards { vault, amount })
                .collect(),
            total_amount: self.total_staker_rewards,
            // token: self.config.wvara_address,
            token: Address::default(),
        };

        RewardsCommitment {
            operators: operators_commitment,
            stakers: stakers_commitment,

            // TODO: add timestamp
            timestamp: 0,
        }
    }

    pub fn extend(&mut self, other: Self) {
        let Rewards {
            operators: other_operators,
            stakers: other_stakers,
            total_operator_rewards: other_total_operator_rewards,
            total_staker_rewards: other_total_staker_rewards,
        } = other;

        for (operator, amount) in other_operators {
            self.operators
                .entry(operator)
                .and_modify(|a| *a += amount)
                .or_insert(amount);
        }

        for (vault, amount) in other_stakers {
            self.stakers
                .entry(vault)
                .and_modify(|a| *a += amount)
                .or_insert(amount);
        }

        self.total_operator_rewards += other_total_operator_rewards;
        self.total_staker_rewards += other_total_staker_rewards;
    }
}

#[cfg_attr(test, derive(Default))]
#[derive(Clone, Debug)]
struct Config {
    pub genesis_timestamp: u64,
    pub era_duration: u64,
    pub slot_duration_secs: u64,
    pub wvara_decimals: U256,

    #[allow(unused)]
    pub wvara_address: Address,
}

#[derive(Clone, derive_more::Debug)]
pub(crate) struct RewardsManager<DB> {
    config: Config,

    #[debug(skip)]
    db: DB,
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

        Ok(Self {
            db,
            config: Config {
                genesis_timestamp,
                era_duration,
                slot_duration_secs: 12u64,
                wvara_decimals: U256::from(18),
                wvara_address,
            },
        })
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

        let mut cumulative_rewards = Rewards {
            // TODO: remove 0 to eras_to_reward.start - 1
            operators: self
                .db
                .operators_rewards_distribution_at(0)
                .ok_or(RewardsError::RewardsDistribution(0))?,
            stakers: Default::default(),
            total_operator_rewards: U256::zero(),
            total_staker_rewards: U256::zero(),
        };
        for era in eras_to_reward {
            let total_era_rewards = self.era_total_rewards(era, block_hash)?;
            let rewards = total_era_rewards.split_into_rewards()?;
            cumulative_rewards.extend(rewards);
        }
        Ok(Some(cumulative_rewards.into_commitment()))
    }

    fn eras_to_reward(&self, block_hash: H256, block_timestamp: u64) -> Result<Option<Range<u64>>> {
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
                if self.should_wait_for_rewards_confirmation(in_block, block_timestamp)? {
                    return Ok(None);
                }

                previous_rewarded
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

    fn should_wait_for_rewards_confirmation(
        &self,
        in_block: H256,
        current_block_ts: u64,
    ) -> Result<bool> {
        let header = self
            .db
            .block_header(in_block)
            .ok_or(RewardsError::BlockHeader(in_block))?;

        let blocks_came = (current_block_ts - header.timestamp) / self.config.slot_duration_secs;
        Ok(blocks_came < REWARDS_CONFIRMATION_BLOCKS_WINDOW)
    }

    fn era_total_rewards(&self, era: u64, chain_head: H256) -> Result<TotalEraRewards<DB>> {
        let mut current_block = chain_head;
        let mut rewards_statistics = BTreeMap::new();
        let mut total_rewards = U256::zero();

        loop {
            let block_header = self
                .db
                .block_header(current_block)
                .ok_or(RewardsError::BlockHeader(current_block))?;
            let block_era = self.era_index(block_header.timestamp);

            if era < block_era {
                current_block = block_header.parent_hash;
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

                let value = U256::from(100) * U256::from(10).pow(self.config.wvara_decimals);

                *operator_rewards += value;
                total_rewards += value;
            }
            current_block = block_header.parent_hash;
        }

        Ok(TotalEraRewards {
            inner: rewards_statistics,
            era,
            db: self.db.clone(),
        })
    }

    pub fn era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.config.genesis_timestamp) / self.config.era_duration
    }
}

mod utils {
    use super::*;

    pub fn build_merkle_tree(rewards: BTreeMap<Address, U256>) -> OzMerkleTree {
        let values = rewards
            .into_iter()
            .map(|(address, amount)| (H160(address.0), amount))
            .collect::<Vec<_>>();
        OzMerkleTree::new(values)
    }
}
