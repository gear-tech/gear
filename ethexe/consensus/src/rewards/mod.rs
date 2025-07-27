use std::{collections::BTreeMap, ops::Range};

use ethexe_common::{
    Address, ToDigest,
    db::{BlockMetaStorageRead, OnChainStorageRead},
    gear::{OperatorRewardsCommitment, RewardsCommitment, StakerRewardsCommitment},
    k256::elliptic_curve::rand_core::block,
};
use gprimitives::{H256, U256};
use rs_merkle::Hasher;
use sha3::Digest;

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
- 0: iter through all previous eras and find latest setted validators
- 1: best case
- 2: iter through all parent blocks and find one of the blocks from parent blocks
*/

#[derive(Debug, Clone)]
pub(crate) struct RewardsConfig {
    pub genesis_timestamp: u64,
    pub era_duration: u64,
    pub wvara_digets: U256,
    pub wvara_address: Address,
}

#[derive(thiserror::Error, Debug)]
pub enum DistributionError {
    #[error("block header not found for: {0:?}")]
    BlockHeaderNotFound(H256),
}
type Result<T> = std::result::Result<T, DistributionError>;

pub(crate) fn rewards_commitment<DB>(
    db: &DB,
    config: &RewardsConfig,
    block_hash: H256,
) -> Result<Option<RewardsCommitment>>
where
    DB: BlockMetaStorageRead + OnChainStorageRead,
{
    let Some(eras_to_reward) = eras_to_reward(db, config, block_hash)? else {
        return Ok(None);
    };

    let (rewards_statistics, total_amount) =
        collect_rewards_statistics(db, config, eras_to_reward, block_hash)?;

    return Ok(None);

    // let rewards_commitment = RewardsCommitment {
    //     operators: operator_rewards_commitment(db, config, eras_to_reward, chain_head)?,
    //     stakers: stakers_rewards_commitment(config)?,
    //     // TODO: add era timestamp
    //     timestamp: 0u64,
    // };

    // Ok(Some(rewards_commitment))
}

fn eras_to_reward<DB>(
    db: &DB,
    config: &RewardsConfig,
    block_hash: H256,
) -> Result<Option<Range<u64>>>
where
    DB: BlockMetaStorageRead + OnChainStorageRead,
{
    let header = db
        .block_header(block_hash)
        .ok_or(DistributionError::BlockHeaderNotFound(block_hash))?;

    let latest_rewarded_era = db.latest_rewarded_era(block_hash).unwrap_or_default();
    let current_era = utils::era_index(config, header.timestamp);

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

fn collect_rewards_statistics<DB>(
    db: &DB,
    config: &RewardsConfig,
    eras: Range<u64>,
    chain_head: H256,
) -> Result<(BTreeMap<Address, U256>, U256)>
where
    DB: BlockMetaStorageRead + OnChainStorageRead,
{
    let mut current_block = chain_head;
    let mut rewards_statistics = BTreeMap::new();
    let mut total_rewards = U256::zero();

    loop {
        let block_header = db
            .block_header(current_block)
            .ok_or(DistributionError::BlockHeaderNotFound(current_block))?;
        let block_era = utils::era_index(config, block_header.timestamp);

        if eras.end <= block_era {
            // We are in the future, no need to continue
            continue;
        }

        if eras.start > block_era {
            // We are in the past, no need to continue
            break;
        }

        let block_validators = validators(current_block);
        for validator in block_validators.iter() {
            let operator_rewards = rewards_statistics.entry(*validator).or_insert(U256::zero());

            let value = U256::from(100) * U256::from(10).pow(config.wvara_digets);

            *operator_rewards += value;
            total_rewards += value;
        }
        current_block = block_header.parent_hash;
    }

    Ok((rewards_statistics, total_rewards))
}

fn split_rewards(mut statistics: BTreeMap<Address, U256>) -> BTreeMap<Address, U256> {
    let mut split_statistics = BTreeMap::new();
    for (address, amount) in statistics {
        let split_amount = amount / U256::from(2); // Example split logic
        split_statistics.insert(address, split_amount);
    }
    split_statistics
}

fn stakers_rewards_commitment(config: &RewardsConfig) -> Result<StakerRewardsCommitment> {
    let stakers_rewards = StakerRewardsCommitment {
        distribution: Vec::new(),
        total_amount: U256::zero(),
        token: config.wvara_address,
    };
    Ok(stakers_rewards)
}

fn validators(_block_hash: H256) -> Vec<Address> {
    vec![]
}

fn operators_merkle_tree(rewards_data: BTreeMap<Address, U256>) -> H256 {
    let leaves = rewards_data
        .into_iter()
        .map(|(address, amount)| {
            let mut hasher = sha3::Keccak256::new();
            hasher.update(&address.0);
            hasher.update(<[u8; 32]>::from(amount));
            hasher.finalize().to_digest().0
        })
        .collect::<Vec<_>>();

    let tree =
        rs_merkle::MerkleTree::<rs_merkle::algorithms::Keccak256>::from_leaves(leaves.as_slice());
    tree.root().expect("Merkle tree should have a root").into()
}

mod utils {
    use super::RewardsConfig;

    pub fn era_index(config: &RewardsConfig, block_ts: u64) -> u64 {
        (block_ts - config.genesis_timestamp) / config.era_duration
    }
}
