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

//! # Utilities Module
//!
//! This module provides utility functions and data structures for handling batch commitments,
//! validation requests, and multi-signature operations in the Ethexe system.

use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData, ValidatorsVec,
    db::{AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, DkgStorageRO, OnChainStorageRO},
    ecdsa::PublicKey,
    gear::{
        AggregatedPublicKey, BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment,
        StateTransition, ValidatorsCommitment,
    },
};
use gprimitives::{CodeId, H256, U256};
use std::collections::HashSet;

/// How often to log warning during chain commitment aggregation
const LOG_WARNING_FREQUENCY: u32 = 10_000;

#[derive(Debug)]
pub struct ValidatorsCommitmentCountMismatch {
    pub package_participants: usize,
    pub elected_validators: usize,
}

impl std::fmt::Display for ValidatorsCommitmentCountMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Elected validators count does not match DKG public key package: \
             package={}, elected={}",
            self.package_participants, self.elected_validators
        )
    }
}

impl std::error::Error for ValidatorsCommitmentCountMismatch {}

#[cfg(test)]
mod test_support {
    use super::*;
    use ethexe_common::{
        Digest, ToDigest, consensus::BatchCommitmentValidationReply, ecdsa::ContractSignature,
    };
    use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
    use parity_scale_codec::{Decode, Encode};
    use std::collections::BTreeMap;

    /// A batch commitment, that has been signed by multiple validators.
    /// This structure manages the collection of signatures from different validators
    /// for a single batch commitment.
    #[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
    pub struct MultisignedBatchCommitment {
        batch: BatchCommitment,
        batch_digest: Digest,
        router_address: Address,
        signatures: BTreeMap<Address, ContractSignature>,
    }

    impl MultisignedBatchCommitment {
        /// Creates a new multisigned batch commitment with an initial signature.
        ///
        /// # Arguments
        /// * `batch` - The batch commitment to be signed
        /// * `signer` - The contract signer used to create signatures
        /// * `pub_key` - The public key of the initial signer
        ///
        /// # Returns
        /// A new `MultisignedBatchCommitment` instance with the initial signature
        pub fn new(
            batch: BatchCommitment,
            signer: &Signer,
            router_address: Address,
            pub_key: PublicKey,
        ) -> Result<Self> {
            let batch_digest = batch.to_digest();
            let signature =
                signer.sign_for_contract_digest(router_address, pub_key, batch_digest, None)?;
            let signatures: BTreeMap<_, _> =
                [(pub_key.to_address(), signature)].into_iter().collect();

            Ok(Self {
                batch,
                batch_digest,
                router_address,
                signatures,
            })
        }

        /// Accepts a validation reply from another validator and adds it's signature.
        ///
        /// # Arguments
        /// * `reply` - The validation reply containing the signature
        /// * `check_origin` - A closure to verify the origin of the signature
        ///
        /// # Returns
        /// Result indicating success or failure of the operation
        pub fn accept_batch_commitment_validation_reply(
            &mut self,
            reply: BatchCommitmentValidationReply,
            check_origin: impl FnOnce(Address) -> Result<()>,
        ) -> Result<()> {
            let BatchCommitmentValidationReply { digest, signature } = reply;

            anyhow::ensure!(digest == self.batch_digest, "Invalid reply digest");

            let origin_public_key = signature.validate(self.router_address, digest)?;

            let origin = origin_public_key.to_address();

            check_origin(origin)?;

            self.signatures.insert(origin, signature);

            Ok(())
        }

        /// Returns a reference to the map of validator addresses to their signatures
        pub fn signatures(&self) -> &BTreeMap<Address, ContractSignature> {
            &self.signatures
        }

        /// Returns a reference to the underlying batch commitment
        pub fn batch(&self) -> &BatchCommitment {
            &self.batch
        }

        /// Consumes the structure and returns its parts
        ///
        /// # Returns
        /// A tuple containing the batch commitment and the map of signatures
        pub fn into_parts(self) -> (BatchCommitment, Vec<ContractSignature>) {
            (self.batch, self.signatures.into_values().collect())
        }
    }
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
#[display("Code not found: {_0}")]
pub struct CodeNotValidatedError(pub CodeId);

pub fn aggregate_code_commitments<DB: CodesStorageRO>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>, CodeNotValidatedError> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => {
                return Err(CodeNotValidatedError(id));
            }
            None => {}
        }
    }

    Ok(commitments)
}

/// Tries to aggregate chain commitment starting from `head_announce_hash` up to the last committed announce
///
/// # NOTE
/// Must be guaranteed by caller that:
/// 1) `head_announce_hash` is computed
/// 2) `head_announce_hash` is successor of `at_block_hash` last committed announce
pub fn try_aggregate_chain_commitment<DB: BlockMetaStorageRO + AnnounceStorageRO>(
    db: &DB,
    at_block_hash: H256,
    head_announce_hash: HashOf<Announce>,
) -> Result<(ChainCommitment, u32)> {
    // TODO #4744: improve squashing - removing redundant state transitions

    if !db.announce_meta(head_announce_hash).computed {
        anyhow::bail!(
            "Head announce {head_announce_hash:?} is not computed, cannot aggregate chain commitment"
        );
    }

    let Some(last_committed_announce_hash) = db.block_meta(at_block_hash).last_committed_announce
    else {
        anyhow::bail!("Last committed announce not found in db for prepared block {at_block_hash}");
    };

    let mut announce_hash = head_announce_hash;
    let mut counter: u32 = 0;
    let mut transitions = vec![];
    while announce_hash != last_committed_announce_hash {
        counter += 1;
        if counter.is_multiple_of(LOG_WARNING_FREQUENCY) {
            tracing::warn!("Aggregating chain commitment: processed {counter} announces so far...");
        }

        if !db.announce_meta(announce_hash).computed {
            // All announces till last committed must be computed.
            // Even fast-sync guarantees that.
            anyhow::bail!("Not computed announce in chain {announce_hash:?}");
        }

        let Some(mut announce_transitions) = db.announce_outcome(announce_hash) else {
            anyhow::bail!("Computed announce {announce_hash:?} outcome not found in db");
        };

        sort_transitions_by_value_to_receive(&mut announce_transitions);

        transitions.push(announce_transitions);

        announce_hash = db
            .announce(announce_hash)
            .ok_or_else(|| anyhow!("Computed announce {announce_hash:?} body not found in db"))?
            .parent;
    }

    Ok((
        ChainCommitment {
            transitions: transitions.into_iter().rev().flatten().collect(),
            head_announce: head_announce_hash,
        },
        counter,
    ))
}

pub fn validators_commitment<DB: DkgStorageRO>(
    db: &DB,
    era: u64,
    validators: ValidatorsVec,
) -> Result<Option<ValidatorsCommitment>> {
    let public_key_package = match db.public_key_package(era) {
        Some(package) => package,
        None => return Ok(None),
    };
    let verifiable_secret_sharing_commitment = match db.dkg_vss_commitment(era) {
        Some(commitment) => commitment,
        None => return Ok(None),
    };
    let package_participants = public_key_package.verifying_shares().len();
    if package_participants != validators.len() {
        return Err(ValidatorsCommitmentCountMismatch {
            package_participants,
            elected_validators: validators.len(),
        }
        .into());
    }

    let public_key_compressed: [u8; 33] = public_key_package
        .verifying_key()
        .serialize()?
        .try_into()
        .map_err(|_| anyhow!("Invalid aggregated public key length"))?;
    let public_key_uncompressed = PublicKey::from_bytes(public_key_compressed)
        .expect("valid aggregated public key")
        .to_uncompressed();
    let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

    let aggregated_public_key = AggregatedPublicKey {
        x: U256::from_big_endian(public_key_x_bytes),
        y: U256::from_big_endian(public_key_y_bytes),
    };

    Ok(Some(ValidatorsCommitment {
        aggregated_public_key,
        verifiable_secret_sharing_commitment,
        validators,
        era_index: era,
    }))
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    chain_commitment: Option<ChainCommitment>,
    code_commitments: Vec<CodeCommitment>,
    validators_commitment: Option<ValidatorsCommitment>,
    rewards_commitment: Option<RewardsCommitment>,
    commitment_delay_limit: u32,
) -> Result<Option<BatchCommitment>> {
    if chain_commitment.is_none()
        && code_commitments.is_empty()
        && validators_commitment.is_none()
        && rewards_commitment.is_none()
    {
        tracing::debug!(
            "No commitments for block {} - skip batch commitment",
            block.hash
        );
        return Ok(None);
    }

    let previous_batch = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(|| {
            anyhow!(
                "Cannot get from db last committed block for block {}",
                block.hash
            )
        })?;

    let expiry = chain_commitment
        .as_ref()
        .map(|c| calculate_batch_expiry(db, block, c.head_announce, commitment_delay_limit))
        .transpose()?
        .flatten()
        .unwrap_or(u8::MAX);

    tracing::trace!(
        "Batch commitment expiry for block {} is {:?}",
        block.hash,
        expiry
    );

    Ok(Some(BatchCommitment {
        block_hash: block.hash,
        timestamp: block.header.timestamp,
        previous_batch,
        expiry,
        chain_commitment,
        code_commitments,
        validators_commitment,
        rewards_commitment,
    }))
}

pub fn calculate_batch_expiry<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    head_announce_hash: HashOf<Announce>,
    commitment_delay_limit: u32,
) -> Result<Option<u8>> {
    let head_announce = db
        .announce(head_announce_hash)
        .ok_or_else(|| anyhow!("Cannot get announce by {head_announce_hash}"))?;

    let head_announce_block_header = db
        .block_header(head_announce.block_hash)
        .ok_or_else(|| anyhow!("block header not found for({})", head_announce.block_hash))?;

    let head_delay = block
        .header
        .height
        .checked_sub(head_announce_block_header.height)
        .ok_or_else(|| {
            anyhow!(
                "Head announce {} has bigger height {}, than batch height {}",
                head_announce_hash,
                head_announce_block_header.height,
                block.header.height,
            )
        })?;

    // Amount of announces which we should check to determine if there are not-base announces in the commitment.
    let Some(announces_to_check_amount) = commitment_delay_limit.checked_sub(head_delay) else {
        // No need to set expiry - head announce is old enough, so cannot contain any not-base announces.
        return Ok(None);
    };

    if announces_to_check_amount == 0 {
        // No need to set expiry - head announce is old enough, so cannot contain any not-base announces.
        return Ok(None);
    }

    let mut oldest_not_base_announce_depth = (!head_announce.is_base()).then_some(0);
    let mut current_announce_hash = head_announce.parent;

    if announces_to_check_amount == 1 {
        // If head announce is not base and older than commitment delay limit - 1, then expiry is only 1.
        return Ok(oldest_not_base_announce_depth.map(|_| 1));
    }

    let last_committed_announce = db
        .block_meta(block.hash)
        .last_committed_announce
        .ok_or_else(|| anyhow!("last committed announce not found for block {}", block.hash))?;

    // from 1 because we have already checked head announce (note announces_to_check_amount > 1)
    for i in 1..announces_to_check_amount {
        if current_announce_hash == last_committed_announce {
            break;
        }

        let current_announce = db
            .announce(current_announce_hash)
            .ok_or_else(|| anyhow!("Cannot get announce by {current_announce_hash}",))?;

        if !current_announce.is_base() {
            oldest_not_base_announce_depth = Some(i);
        }

        current_announce_hash = current_announce.parent;
    }

    Ok(oldest_not_base_announce_depth
        .map(|depth| announces_to_check_amount - depth)
        .map(TryInto::try_into)
        .transpose()?)
}

pub fn has_duplicates<T: std::hash::Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

// TODO #4553: temporary implementation, should be improved
/// Returns block producer for time slot. Next slot is the next validator in the list.
pub const fn block_producer_index(validators_amount: usize, slot: u64) -> usize {
    (slot % validators_amount as u64) as usize
}

/// Calculates the producer address for a given slot based on the validators and timestamp.
///
/// # Arguments
/// * `validators` - A list of validator addresses
/// * `timestamp` - The timestamp to determine the slot (in seconds)
/// * `slot_duration` - The duration of each slot (in seconds)
///
/// # Returns
/// The address of the producer for the given timestamp slot.
pub fn block_producer_for(
    validators: &ValidatorsVec,
    timestamp: u64,
    slot_duration: u64,
) -> Address {
    let slot = timestamp / slot_duration;
    let index = block_producer_index(validators.len(), slot);
    validators
        .get(index)
        .cloned()
        .unwrap_or_else(|| unreachable!("index must be valid"))
}

pub fn sort_transitions_by_value_to_receive(transitions: &mut [StateTransition]) {
    transitions.sort_by(|lhs, rhs| {
        rhs.value_to_receive_negative_sign
            .cmp(&lhs.value_to_receive_negative_sign)
    });
}

#[cfg(test)]
mod tests {
    use super::{test_support::MultisignedBatchCommitment, *};
    use crate::mock::*;
    use ethexe_common::{ToDigest, consensus::BatchCommitmentValidationReply, db::*, mock::*};
    use ethexe_db::Database;
    use gsigner::secp256k1::Secp256k1SignerExt;

    const ADDRESS: Address = Address([42; 20]);

    #[test]
    fn block_producer_index_matches_modulo_across_slots() {
        let cases = [(1, 0, 0), (3, 2, 2), (3, 3, 0), (5, 7, 2), (5, 14, 4)];

        for (validators_amount, slot, expected) in cases {
            assert_eq!(block_producer_index(validators_amount, slot), expected);
        }
    }

    #[test]
    fn block_producer_for_uses_slot_duration() {
        let validators = vec![
            Address::from([1; 20]),
            Address::from([2; 20]),
            Address::from([3; 20]),
        ]
        .try_into()
        .unwrap();
        let slot_duration = 5;

        let cases = [(0, 0), (4, 0), (5, 1), (9, 1), (10, 2), (15, 0)];

        for (timestamp, expected_index) in cases {
            let producer = block_producer_for(&validators, timestamp, slot_duration);
            assert_eq!(producer, validators[expected_index]);
        }
    }

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let multisigned_batch =
            MultisignedBatchCommitment::new(batch.clone(), &signer, ADDRESS, pub_key)
                .expect("Failed to create multisigned batch commitment");

        assert_eq!(multisigned_batch.batch(), &batch);
        assert_eq!(multisigned_batch.signatures().len(), 1);
    }

    #[test]
    fn accept_batch_commitment_validation_reply() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let digest = multisigned_batch.batch().to_digest();
        let signature = signer
            .sign_for_contract_digest(ADDRESS, other_pub_key, digest, None)
            .unwrap();
        let reply = BatchCommitmentValidationReply { digest, signature };

        multisigned_batch
            .accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        assert_eq!(multisigned_batch.signatures().len(), 2);

        // Attempt to add the same reply again
        multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        // Ensure the number of signatures has not increased
        assert_eq!(multisigned_batch.signatures().len(), 2);

        let (batch, signatures) = multisigned_batch.into_parts();
        assert_eq!(batch.to_digest(), digest);
        assert_eq!(signatures.len(), 2);
    }

    #[test]
    fn reject_validation_reply_with_incorrect_digest() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let incorrect_digest = [1, 2, 3].to_digest();
        let reply = BatchCommitmentValidationReply {
            digest: incorrect_digest,
            signature: signer
                .sign_for_contract_digest(ADDRESS, pub_key, incorrect_digest, None)
                .unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures().len(), 1);
    }

    #[test]
    fn check_origin_closure_behavior() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let digest = multisigned_batch.batch().to_digest();
        let signature = signer
            .sign_for_contract_digest(ADDRESS, other_pub_key, digest, None)
            .unwrap();
        let reply = BatchCommitmentValidationReply { digest, signature };

        // Case 1: check_origin allows the origin
        let result =
            multisigned_batch.accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()));
        assert!(result.is_ok());
        assert_eq!(multisigned_batch.signatures().len(), 2);

        // Case 2: check_origin rejects the origin
        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| {
            anyhow::bail!("Origin not allowed")
        });
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures().len(), 2);
    }

    #[test]
    fn test_aggregate_chain_commitment() {
        {
            // Valid case, two transitions in the chain, but only one must be included
            let db = Database::memory();
            let chain = BlockChain::mock(10)
                .tap_mut(|chain| {
                    chain
                        .block_top_announce_mut(3)
                        .as_computed_mut()
                        .outcome
                        .push(StateTransition::mock(()));
                    chain
                        .block_top_announce_mut(5)
                        .as_computed_mut()
                        .outcome
                        .push(StateTransition::mock(()));
                    chain.blocks[10].as_prepared_mut().last_committed_announce =
                        chain.block_top_announce_hash(3);
                })
                .setup(&db);
            let block = chain.blocks[10].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(9);

            let (commitment, counter) =
                try_aggregate_chain_commitment(&db, block.hash, head_announce_hash).unwrap();
            assert_eq!(commitment.head_announce, head_announce_hash);
            assert_eq!(commitment.transitions.len(), 1);
            assert_eq!(counter, 6);
        }

        {
            // head announce not computed
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.block_top_announce_mut(3).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);

            try_aggregate_chain_commitment(&db, block.hash, head_announce_hash).unwrap_err();
        }

        {
            // announce in chain not computed
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.block_top_announce_mut(2).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);

            try_aggregate_chain_commitment(&db, block.hash, head_announce_hash).unwrap_err();
        }

        {
            // last committed announce missing in block meta
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.blocks[3].prepared = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(2);

            try_aggregate_chain_commitment(&db, block.hash, head_announce_hash).unwrap_err();
        }
    }

    #[test]
    fn test_aggregate_code_commitments() {
        let db = Database::memory();
        let codes = vec![CodeId::from([1; 32]), CodeId::from([2; 32])];

        // Test with valid codes
        db.set_code_valid(codes[0], true);
        db.set_code_valid(codes[1], false);

        let commitments = aggregate_code_commitments(&db, codes.clone(), false).unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        let commitments =
            aggregate_code_commitments(&db, vec![codes[0], CodeId::from([3; 32]), codes[1]], false)
                .unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        aggregate_code_commitments(&db, vec![CodeId::from([3; 32])], true).unwrap_err();
    }

    #[test]
    fn test_has_duplicates() {
        let data = vec![1, 2, 3, 4, 5];
        assert!(!has_duplicates(&data));

        let data = vec![1, 2, 3, 4, 5, 3];
        assert!(has_duplicates(&data));
    }

    #[test]
    fn test_batch_expiry_calculation() {
        {
            let db = Database::memory();
            let chain = BlockChain::mock(1).setup(&db);
            let block = chain.blocks[1].to_simple();
            let expiry =
                calculate_batch_expiry(&db, &block, db.top_announce_hash(block.hash), 5).unwrap();
            assert!(expiry.is_none(), "Expiry should be None");
        }

        {
            let db = Database::memory();
            let chain = BlockChain::mock(10)
                .tap_mut(|c| {
                    c.block_top_announce_mut(10).announce.gas_allowance = Some(10);
                    c.blocks[10].as_prepared_mut().announces =
                        Some([c.block_top_announce(10).announce.to_hash()].into());
                })
                .setup(&db);

            let block = chain.blocks[10].to_simple();
            let expiry =
                calculate_batch_expiry(&db, &block, db.top_announce_hash(block.hash), 100).unwrap();
            assert_eq!(
                expiry,
                Some(100),
                "Expiry should be 100 as there is one not-base announce"
            );
        }

        {
            let db = Database::memory();
            let batch = prepare_chain_for_batch_commitment(&db);
            let block = db.simple_block_data(batch.block_hash);
            let expiry = calculate_batch_expiry(
                &db,
                &block,
                batch.chain_commitment.as_ref().unwrap().head_announce,
                3,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                expiry, batch.expiry,
                "Expiry should match the one in the batch commitment"
            );
        }
    }
}
