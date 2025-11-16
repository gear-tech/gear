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
    Address, Announce, Digest, HashOf, SimpleBlockData, ToDigest, ValidatorsVec,
    consensus::BatchCommitmentValidationReply,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, BlockMetaStorageRW,
        CodesStorageRO, OnChainStorageRO,
    },
    ecdsa::{ContractSignature, PublicKey},
    gear::{
        AggregatedPublicKey, BatchCommitment, ChainCommitment, CodeCommitment, Message,
        RewardsCommitment, StateTransition, ValidatorsCommitment, ValueClaim,
    },
};
use ethexe_signer::Signer;
use gprimitives::{ActorId, CodeId, H256, U256};
use parity_scale_codec::{Decode, Encode};
use rand::SeedableRng;
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{self, IdentifierList},
};
use std::{
    collections::{BTreeMap, HashSet, VecDeque, btree_map::Entry},
    mem,
};

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
        let signature = signer.sign_for_contract(router_address, pub_key, batch_digest)?;
        let signatures: BTreeMap<_, _> = [(pub_key.to_address(), signature)].into_iter().collect();

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

        let origin = signature
            .validate(self.router_address, digest)?
            .to_address();

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
    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, ContractSignature>) {
        (self.batch, self.signatures)
    }
}

pub fn aggregate_code_commitments<DB: CodesStorageRO>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => {
                return Err(anyhow::anyhow!("Code status not found in db: {id}"));
            }
            None => {}
        }
    }

    Ok(commitments)
}

pub fn aggregate_chain_commitment<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    head_announce: HashOf<Announce>,
    fail_if_not_computed: bool,
    max_deepness: Option<u32>,
) -> Result<Option<(ChainCommitment, u32)>> {
    // TODO #4744: improve squashing - removing redundant state transitions

    let block_hash = db
        .announce(head_announce)
        .ok_or_else(|| anyhow!("Cannot get announce from db for head {head_announce}"))?
        .block_hash;

    let last_committed_head = db
        .block_meta(block_hash)
        .last_committed_announce
        .ok_or_else(|| {
            anyhow!("Cannot get from db last committed head for block {head_announce}")
        })?;

    let mut announce_hash = head_announce;
    let mut counter: u32 = 0;
    let mut per_block_transitions = Vec::new();
    let mut expected_parent: Option<HashOf<Announce>> = None;
    while announce_hash != last_committed_head {
        if let Some(expected) = expected_parent {
            debug_assert_eq!(
                expected, announce_hash,
                "announce chain is broken while squashing commitments"
            );
        }
        if max_deepness.map(|d| counter >= d).unwrap_or(false) {
            return Err(anyhow!(
                "Chain commitment is too deep: {block_hash} at depth {counter}"
            ));
        }

        counter += 1;

        if !db.announce_meta(announce_hash).computed {
            // This can happen when validator syncs from p2p network and skips some old blocks.
            if fail_if_not_computed {
                return Err(anyhow!("Block {block_hash} is not computed"));
            } else {
                return Ok(None);
            }
        }

        let mut announce_transitions = db
            .announce_outcome(announce_hash)
            .ok_or_else(|| anyhow!("Cannot get from db outcome for computed block {block_hash}"))?;

        sort_transitions_by_value_to_receive(&mut announce_transitions);

        per_block_transitions.push(announce_transitions);

        let parent = db
            .announce(announce_hash)
            .ok_or_else(|| anyhow!("Cannot get from db header for computed block {block_hash}"))?
            .parent;
        expected_parent = Some(parent);
        announce_hash = parent;
    }

    let mut aggregations: BTreeMap<ActorId, ActorAggregation> = BTreeMap::new();
    for transitions in per_block_transitions.into_iter().rev() {
        for transition in transitions {
            match aggregations.entry(transition.actor_id) {
                Entry::Vacant(entry) => {
                    entry.insert(ActorAggregation::new(transition));
                }
                Entry::Occupied(mut entry) => {
                    entry.get_mut().absorb(transition);
                }
            }
        }
    }

    let squashed_transitions = aggregations.into_values().map(|aggregation| aggregation.finish()).collect();

    Ok(Some((
        ChainCommitment {
            transitions: squashed_transitions,
            head_announce,
        },
        counter,
    )))
}

struct ActorAggregation {
    newest: StateTransition,
    messages: Vec<Message>,
    value_claims: Vec<ValueClaim>,
    total_value: u128,
    exit_inheritor: Option<ActorId>,
}

impl ActorAggregation {
    fn new(mut transition: StateTransition) -> Self {
        let messages = mem::take(&mut transition.messages);
        let value_claims = mem::take(&mut transition.value_claims);
        let exit_inheritor = transition.exited.then_some(transition.inheritor);

        Self {
            total_value: transition.value_to_receive,
            newest: transition,
            messages,
            value_claims,
            exit_inheritor,
        }
    }

    fn absorb(&mut self, mut transition: StateTransition) {
        self.messages.append(&mut transition.messages);
        self.value_claims.append(&mut transition.value_claims);
        self.total_value = self.total_value.saturating_add(transition.value_to_receive);
        if transition.exited {
            self.exit_inheritor = Some(transition.inheritor);
        }
        self.newest = transition;
    }

    fn finish(mut self) -> StateTransition {
        if let Some(inheritor) = self.exit_inheritor {
            self.newest.inheritor = inheritor;
            self.newest.exited = true;
        }
        self.newest.messages = self.messages;
        self.newest.value_claims = self.value_claims;
        self.newest.value_to_receive = self.total_value;
        self.newest
    }
}

// TODO(kuzmindev): this is a temporal solution. In future need to impelement DKG algorithm.
pub fn validators_commitment(era: u64, validators: ValidatorsVec) -> Result<ValidatorsCommitment> {
    let validators_identifiers = validators
        .iter()
        .map(|validator| {
            let mut bytes = [0u8; 32];
            bytes[12..32].copy_from_slice(&validator.0);
            Identifier::deserialize(&bytes).unwrap()
        })
        .collect::<Vec<_>>();

    let identifiers = IdentifierList::Custom(&validators_identifiers);

    let rng = rand_chacha::ChaCha8Rng::from_seed([1u8; 32]);

    let (mut secret_shares, public_key_package) =
        keys::generate_with_dealer(validators.len() as u16, 1, identifiers, rng).unwrap();

    let verifiable_secret_sharing_commitment = secret_shares
        .pop_first()
        .map(|(_key, value)| value.commitment().clone())
        .expect("Expect at least one identifier");

    let public_key_compressed: [u8; 33] = public_key_package
        .verifying_key()
        .serialize()?
        .try_into()
        .unwrap();
    let public_key_uncompressed = PublicKey(public_key_compressed).to_uncompressed();
    let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

    let aggregated_public_key = AggregatedPublicKey {
        x: U256::from_big_endian(public_key_x_bytes),
        y: U256::from_big_endian(public_key_y_bytes),
    };

    Ok(ValidatorsCommitment {
        aggregated_public_key,
        verifiable_secret_sharing_commitment,
        validators,
        era_index: era,
    })
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    chain_commitment: Option<ChainCommitment>,
    code_commitments: Vec<CodeCommitment>,
    validators_commitment: Option<ValidatorsCommitment>,
    rewards_commitment: Option<RewardsCommitment>,
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

    let last_committed = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(|| {
            anyhow!(
                "Cannot get from db last committed block for block {}",
                block.hash
            )
        })?;

    Ok(Some(BatchCommitment {
        block_hash: block.hash,
        timestamp: block.header.timestamp,
        previous_batch: last_committed,
        chain_commitment,
        code_commitments,
        validators_commitment,
        rewards_commitment,
    }))
}

pub fn has_duplicates<T: std::hash::Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

/// Finds the block with the earliest timestamp that is still within the specified election period.
pub fn election_block_in_era<DB: OnChainStorageRO>(
    db: &DB,
    mut block: SimpleBlockData,
    election_ts: u64,
) -> Result<SimpleBlockData> {
    if block.header.timestamp < election_ts {
        anyhow::bail!("election not reached yet");
    }

    loop {
        let parent_header = db.block_header(block.header.parent_hash).ok_or(anyhow!(
            "block header not found for({})",
            block.header.parent_hash
        ))?;
        if parent_header.timestamp < election_ts {
            break;
        }

        block = SimpleBlockData {
            hash: block.header.parent_hash,
            header: parent_header,
        };
    }

    Ok(block)
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

// NOTE: this is temporary main line announce, will be smarter in future
/// Returns announce hash which is supposed to be the announce
/// from main announces chain for this node.
/// Used to identify parent announce when creating announce for new block,
/// or accepting announce from producer.
pub fn parent_main_line_announce<DB: BlockMetaStorageRO>(
    db: &DB,
    parent_hash: H256,
) -> Result<HashOf<Announce>> {
    db.block_meta(parent_hash)
        .announces
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| anyhow!("No announces found for {parent_hash} in block meta storage"))
}

// TODO #4813: support announce branching and mortality
/// Creates announces chain till the specified block, from the nearest ancestor without announces,
/// by appending base announces.
pub fn propagate_announces_for_skipped_blocks<
    DB: BlockMetaStorageRO + BlockMetaStorageRW + AnnounceStorageRW + OnChainStorageRO,
>(
    db: &DB,
    block_hash: H256,
) -> Result<()> {
    let mut current_block_hash = block_hash;
    let mut blocks = VecDeque::new();
    // tries to found a block with at least one announce
    let mut announce_hash = loop {
        let announce_hash = db
            .block_meta(current_block_hash)
            .announces
            .into_iter()
            .flatten()
            .next();

        if let Some(announce_hash) = announce_hash {
            break announce_hash;
        }

        blocks.push_front(current_block_hash);
        current_block_hash = db
            .block_header(current_block_hash)
            .ok_or_else(|| anyhow!("Block header not found for {current_block_hash}"))?
            .parent_hash;
    };

    // the newest block with announce is found, create announces chain till the target block
    for block_hash in blocks {
        // TODO #4814: hack - use here with default gas announce to avoid unknown announces in tests,
        // this will be fixed by unknown announces handling later
        let announce = Announce::with_default_gas(block_hash, announce_hash);
        announce_hash = db.set_announce(announce);
        db.mutate_block_meta(block_hash, |meta| {
            meta.announces.get_or_insert_default().insert(announce_hash);
        });
    }

    Ok(())
}

fn sort_transitions_by_value_to_receive(transitions: &mut [StateTransition]) {
    transitions.sort_by(|lhs, rhs| {
        rhs.value_to_receive_negative_sign
            .cmp(&lhs.value_to_receive_negative_sign)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use ethexe_common::{db::*, mock::*};
    use ethexe_db::Database;

    const ADDRESS: Address = Address([42; 20]);

    #[test]
    fn block_producer_index_calculates_correct_index() {
        let validators_amount = 5;
        let slot = 7;
        let index = block_producer_index(validators_amount, slot);
        assert_eq!(index, 2);
    }

    #[test]
    fn producer_for_calculates_correct_producer() {
        let validators = vec![
            Address::from([1; 20]),
            Address::from([2; 20]),
            Address::from([3; 20]),
        ]
        .try_into()
        .unwrap();
        let timestamp = 10;

        let producer = block_producer_for(&validators, timestamp, 1);
        assert_eq!(producer, validators[timestamp as usize % validators.len()]);
    }

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let multisigned_batch =
            MultisignedBatchCommitment::new(batch.clone(), &signer, ADDRESS, pub_key)
                .expect("Failed to create multisigned batch commitment");

        assert_eq!(multisigned_batch.batch, batch);
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn accept_batch_commitment_validation_reply() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        multisigned_batch
            .accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Attempt to add the same reply again
        multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        // Ensure the number of signatures has not increased
        assert_eq!(multisigned_batch.signatures.len(), 2);
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
                .sign_for_contract(ADDRESS, pub_key, incorrect_digest)
                .unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn check_origin_closure_behavior() {
        let batch = BatchCommitment::mock(());

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        // Case 1: check_origin allows the origin
        let result =
            multisigned_batch.accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()));
        assert!(result.is_ok());
        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Case 2: check_origin rejects the origin
        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| {
            anyhow::bail!("Origin not allowed")
        });
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 2);
    }

    #[test]
    fn test_aggregate_chain_commitment() {
        let db = Database::memory();
        let BatchCommitment { block_hash, .. } = prepare_chain_for_batch_commitment(&db);
        let announce = db.top_announce_hash(block_hash);

        let (commitment, counter) = aggregate_chain_commitment(&db, announce, false, None)
            .unwrap()
            .unwrap();
        assert_eq!(commitment.head_announce, announce);
        assert_eq!(commitment.transitions.len(), 4);
        assert_eq!(counter, 3);

        let (commitment, counter) = aggregate_chain_commitment(&db, announce, true, None)
            .unwrap()
            .unwrap();
        assert_eq!(commitment.head_announce, announce);
        assert_eq!(commitment.transitions.len(), 4);
        assert_eq!(counter, 3);

        aggregate_chain_commitment(&db, announce, false, Some(2)).unwrap_err();
        aggregate_chain_commitment(&db, announce, true, Some(2)).unwrap_err();

        db.mutate_announce_meta(announce, |meta| meta.computed = false);
        assert!(
            aggregate_chain_commitment(&db, announce, false, None)
                .unwrap()
                .is_none()
        );
        aggregate_chain_commitment(&db, announce, true, None).unwrap_err();
    }

    #[test]
    fn test_chain_commitment_squashing() {
        use ethexe_common::gear::Message;

        let db = Database::memory();
        let BatchCommitment { block_hash, .. } = prepare_chain_for_batch_commitment(&db);
        let head = db.top_announce_hash(block_hash);
        let parent = db.announce(head).expect("announce exists").parent;

        let actor = ActorId::from([7; 32]);
        let inheritor_old = ActorId::from([8; 32]);
        let inheritor_new = ActorId::from([9; 32]);

        let m1 = Message {
            id: Default::default(),
            destination: inheritor_old,
            payload: b"old".to_vec(),
            value: 1,
            reply_details: None,
            call: false,
        };
        let m2 = Message {
            id: Default::default(),
            destination: inheritor_new,
            payload: b"new".to_vec(),
            value: 2,
            reply_details: None,
            call: false,
        };

        db.set_announce_outcome(
            parent,
            vec![StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: true,
                inheritor: inheritor_old,
                value_to_receive: 1,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m1.clone()],
            }],
        );

        db.set_announce_outcome(
            head,
            vec![StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: true,
                inheritor: inheritor_new,
                value_to_receive: 2,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m2.clone()],
            }],
        );

        let (commitment, _) = aggregate_chain_commitment(&db, head, false, None)
            .unwrap()
            .unwrap();

        assert!(!commitment.transitions.is_empty());
        let st = commitment
            .transitions
            .iter()
            .find(|transition| transition.actor_id == actor)
            .expect("actor transition");
        assert_eq!(st.new_state_hash, H256::from([2; 32]));
        assert!(st.exited);
        assert_eq!(st.inheritor, inheritor_new);
        assert_eq!(st.messages, vec![m1, m2]);
        assert_eq!(st.value_to_receive, 3);
    }

    #[test]
    fn test_chain_commitment_empty_when_already_committed() {
        let db = Database::memory();
        let BatchCommitment { block_hash, .. } = prepare_chain_for_batch_commitment(&db);
        let head = db.top_announce_hash(block_hash);

        db.mutate_block_meta(block_hash, |meta| meta.last_committed_announce = Some(head));

        let (commitment, counter) = aggregate_chain_commitment(&db, head, true, None)
            .unwrap()
            .unwrap();

        assert_eq!(counter, 0);
        assert_eq!(commitment.head_announce, head);
        assert!(commitment.transitions.is_empty());
    }

    #[test]
    fn test_squashing_value_saturating_add() {
        let db = Database::memory();
        let BatchCommitment { block_hash, .. } = prepare_chain_for_batch_commitment(&db);
        let head = db.top_announce_hash(block_hash);
        let parent = db.announce(head).expect("announce exists").parent;

        let actor = ActorId::from([5; 32]);

        db.set_announce_outcome(
            parent,
            vec![StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 42,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            }],
        );

        db.set_announce_outcome(
            head,
            vec![StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: u128::MAX - 10,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            }],
        );

        let (commitment, _) = aggregate_chain_commitment(&db, head, false, None)
            .unwrap()
            .unwrap();

        let st = &commitment
            .transitions
            .iter()
            .find(|transition| transition.actor_id == actor)
            .expect("actor transition");
        assert_eq!(st.value_to_receive, u128::MAX);
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
}
