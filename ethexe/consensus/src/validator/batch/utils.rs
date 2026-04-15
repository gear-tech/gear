// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::validator::batch::{filler::BatchFiller, types::BatchParts};

use super::types::CodeNotValidatedError;

use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, OnChainStorageRO},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition, ValueClaim,
    },
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{HashMap, hash_map::Entry};

pub fn collect_not_committed_predecessors<DB: AnnounceStorageRO + BlockMetaStorageRO>(
    db: &DB,
    last_committed_announce_hash: HashOf<Announce>,
    announce_hash: HashOf<Announce>,
) -> Result<Vec<HashOf<Announce>>> {
    let mut announces = Vec::new();
    let mut current_announce = announce_hash;

    // Maybe remove this loop to prevent infinite searching
    while current_announce != last_committed_announce_hash {
        if !db.announce_meta(current_announce).computed {
            // All announces till last committed must be computed.
            // Even fast-sync guarantees that.
            bail!("Not computed announce in chain {current_announce:?}")
        }

        announces.push(current_announce);
        current_announce = db
            .announce(current_announce)
            .ok_or_else(|| anyhow!("Computed announce {current_announce:?} body not found in db"))?
            .parent;
    }

    Ok(announces.into_iter().rev().collect())
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO + OnChainStorageRO + AnnounceStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    batch_parts: BatchParts,
    commitment_delay_limit: u32,
) -> Result<Option<BatchCommitment>> {
    let BatchParts {
        chain_commitment,
        validators_commitment,
        code_commitments,
        rewards_commitment,
    } = batch_parts;

    let block_hash = block.hash;
    if chain_commitment.is_none()
        && code_commitments.is_empty()
        && validators_commitment.is_none()
        && rewards_commitment.is_none()
    {
        tracing::debug!("No commitments for block {block_hash} - skip batch commitment",);
        return Ok(None);
    }

    let previous_batch = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(
            || anyhow!("Cannot get from db last committed block for block {block_hash}",),
        )?;

    let expiry = chain_commitment
        .as_ref()
        .map(|c| calculate_batch_expiry(db, block, c.head_announce, commitment_delay_limit))
        .transpose()?
        .flatten()
        .unwrap_or(u8::MAX);

    tracing::trace!("Batch commitment expiry for block {block_hash} is {expiry:?}",);

    Ok(Some(BatchCommitment {
        block_hash,
        timestamp: block.header.timestamp,
        previous_batch,
        expiry,
        chain_commitment,
        code_commitments,
        validators_commitment,
        rewards_commitment,
    }))
}

pub fn aggregate_code_commitments<DB: CodesStorageRO>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>, CodeNotValidatedError> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => return Err(CodeNotValidatedError(id)),
            None => {}
        }
    }

    Ok(commitments)
}

pub fn try_include_chain_commitment<DB: BlockMetaStorageRO + AnnounceStorageRO>(
    db: &DB,
    at_block: H256,
    head_announce_hash: HashOf<Announce>,
    batch_filler: &mut BatchFiller,
) -> Result<(HashOf<Announce>, u32)> {
    if !db.announce_meta(head_announce_hash).computed {
        anyhow::bail!(
            "Head announce {head_announce_hash:?} is not computed, cannot aggregate chain commitment"
        );
    }

    let Some(last_committed_announce) = db.block_meta(at_block).last_committed_announce else {
        anyhow::bail!("Last committed announce not found in db for prepared block: {at_block}",);
    };

    let pending = super::utils::collect_not_committed_predecessors(
        &db,
        last_committed_announce,
        head_announce_hash,
    )?;

    let final_announce = pending.last().copied().unwrap_or(head_announce_hash);
    let max_depth = pending.len() as u32;

    for (depth, announce_hash) in pending.into_iter().enumerate() {
        let transitions = super::utils::announce_transitions(&db, announce_hash)?;
        let commitment = ChainCommitment {
            head_announce: announce_hash,
            transitions,
        };

        if let Err(err) = batch_filler.include_chain_commitment(commitment, depth as u32) {
            tracing::trace!(
                "failed to include chain commitment for announce({announce_hash}) because of error={err}"
            );
            return Ok((announce_hash, depth as u32));
        }
    }
    Ok((final_announce, max_depth))
}

pub fn announce_transitions<DB: AnnounceStorageRO>(
    db: &DB,
    announce_hash: HashOf<Announce>,
) -> Result<Vec<StateTransition>> {
    let Some(mut announce_transitions) = db.announce_outcome(announce_hash) else {
        anyhow::bail!("Computed announce {announce_hash:?} outcome not found in db");
    };

    sort_transitions_by_value_to_receive(&mut announce_transitions);
    Ok(announce_transitions)
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

/// Squashes transitions for the same actor into a single transition per actor.
///
/// For each actor, the newest transition (last in chronological order) provides the
/// `new_state_hash`. Messages, value claims, and `value_to_receive` are accumulated
/// from all transitions. If any transition marks the actor as exited, the resulting
/// inheritor is taken from the newest exit transition. The returned transitions are
/// stably re-sorted so negative `value_to_receive` entries run before non-negative
/// ones during on-chain execution, allowing the router to collect outgoing value
/// before funding receivers in the same batch.
pub fn squash_transitions_by_actor(transitions: Vec<StateTransition>) -> Vec<StateTransition> {
    let mut positions = HashMap::new();
    let mut aggregations = Vec::new();

    for transition in transitions {
        match positions.entry(transition.actor_id) {
            Entry::Vacant(entry) => {
                entry.insert(aggregations.len());
                aggregations.push(ActorAggregation::new(transition));
            }
            Entry::Occupied(entry) => {
                aggregations[*entry.get()].absorb(transition);
            }
        }
    }

    let mut squashed = aggregations
        .into_iter()
        .map(|aggregation| aggregation.finish())
        .collect::<Vec<_>>();
    sort_transitions_by_value_to_receive(&mut squashed);
    squashed
}

struct ActorAggregation {
    newest: StateTransition,
    messages: Vec<Message>,
    value_claims: Vec<ValueClaim>,
    value_to_receive: SignedMagnitude,
    exit_inheritor: Option<ActorId>,
}

impl ActorAggregation {
    fn new(mut transition: StateTransition) -> Self {
        let messages = std::mem::take(&mut transition.messages);
        let value_claims = std::mem::take(&mut transition.value_claims);
        let exit_inheritor = transition.exited.then_some(transition.inheritor);

        Self {
            value_to_receive: SignedMagnitude::new(
                transition.value_to_receive,
                transition.value_to_receive_negative_sign,
            ),
            newest: transition,
            messages,
            value_claims,
            exit_inheritor,
        }
    }

    fn absorb(&mut self, mut transition: StateTransition) {
        let actor_id = transition.actor_id;
        self.messages.append(&mut transition.messages);
        self.value_claims.append(&mut transition.value_claims);
        self.value_to_receive.add_assign(
            SignedMagnitude::new(
                transition.value_to_receive,
                transition.value_to_receive_negative_sign,
            ),
            actor_id,
        );
        if transition.exited {
            self.exit_inheritor = Some(transition.inheritor);
        }
        self.newest = transition;
    }

    fn finish(self) -> StateTransition {
        let SignedMagnitude {
            value: value_to_receive,
            negative: value_to_receive_negative_sign,
        } = self.value_to_receive;

        StateTransition {
            actor_id: self.newest.actor_id,
            new_state_hash: self.newest.new_state_hash,
            exited: self.exit_inheritor.is_some(),
            inheritor: self.exit_inheritor.unwrap_or(self.newest.inheritor),
            value_to_receive,
            value_to_receive_negative_sign,
            value_claims: self.value_claims,
            messages: self.messages,
        }
    }
}

/// Internal signed-magnitude helper for `StateTransition::value_to_receive`.
///
/// Consensus stores the transfer amount as `(u128, negative_sign)` instead of a
/// signed integer to keep the on-chain representation cheaper. Squashing needs
/// signed arithmetic, so this helper performs addition directly on that wire
/// format:
/// - zero is always normalized to `negative = false`
/// - equal signs use checked addition
/// - opposite signs subtract the smaller magnitude from the larger one and keep
///   the sign of the larger magnitude
#[derive(Clone, Copy)]
struct SignedMagnitude {
    value: u128,
    negative: bool,
}

impl SignedMagnitude {
    fn new(value: u128, negative: bool) -> Self {
        Self {
            value,
            negative: value != 0 && negative,
        }
    }

    fn add_assign(&mut self, other: Self, actor_id: ActorId) {
        match self.negative == other.negative {
            true => {
                self.value = self.value.checked_add(other.value).unwrap_or_else(|| {
                    panic!("squashed transition value overflow for actor {actor_id:?}")
                });
            }
            false => match self.value.cmp(&other.value) {
                std::cmp::Ordering::Greater => {
                    self.value -= other.value;
                }
                std::cmp::Ordering::Equal => {
                    self.value = 0;
                    self.negative = false;
                }
                std::cmp::Ordering::Less => {
                    self.value = other.value - self.value;
                    self.negative = other.negative;
                }
            },
        }
    }
}

pub fn sort_transitions_by_value_to_receive(transitions: &mut [StateTransition]) {
    // `false < true`, so invert the key to keep transitions that return value to
    // the router ahead of transitions that receive value from it.
    transitions.sort_by_key(|transition| !transition.value_to_receive_negative_sign);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        validator::batch::{BatchLimits, filler::BatchFiller},
    };
    use ethexe_common::{
        COMMITMENT_DELAY_LIMIT, DEFAULT_BLOCK_GAS_LIMIT,
        consensus::DEFAULT_CHAIN_DEEPNESS_THRESHOLD, db::*, mock::*,
    };
    use ethexe_db::Database;

    const BATCH_LIMITS: BatchLimits = BatchLimits {
        chain_deepness_threshold: DEFAULT_CHAIN_DEEPNESS_THRESHOLD,
        commitment_delay_limit: COMMITMENT_DELAY_LIMIT,
        batch_size_limit: DEFAULT_BLOCK_GAS_LIMIT,
    };

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

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            let (_, deepness) = try_include_chain_commitment(
                &db,
                block.hash,
                head_announce_hash,
                &mut batch_filler,
            )
            .unwrap();
            let commitment = batch_filler.into_parts().chain_commitment.unwrap();

            assert_eq!(commitment.head_announce, head_announce_hash);
            assert_eq!(commitment.transitions.len(), 1);
            assert_eq!(deepness, 6);
        }

        {
            // head announce not computed
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.block_top_announce_mut(3).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);
            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);

            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
        }

        {
            // announce in chain not computed
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.block_top_announce_mut(2).computed = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(3);

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
        }

        {
            // last committed announce missing in block meta
            let db = Database::memory();
            let chain = BlockChain::mock(3)
                .tap_mut(|chain| chain.blocks[3].prepared = None)
                .setup(&db);
            let block = chain.blocks[3].to_simple();
            let head_announce_hash = chain.block_top_announce_hash(2);

            let mut batch_filler = BatchFiller::new(BATCH_LIMITS);
            try_include_chain_commitment(&db, block.hash, head_announce_hash, &mut batch_filler)
                .unwrap_err();
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

    #[test]
    fn test_squash_transitions_by_actor() {
        use ethexe_common::gear::Message;

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

        let transitions = vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: true,
                inheritor: inheritor_old,
                value_to_receive: 1,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m1.clone()],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: true,
                inheritor: inheritor_new,
                value_to_receive: 2,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m2.clone()],
            },
        ];

        let squashed = squash_transitions_by_actor(transitions);
        assert_eq!(squashed.len(), 1);

        let st = &squashed[0];
        assert_eq!(st.actor_id, actor);
        assert_eq!(st.new_state_hash, H256::from([2; 32]));
        assert!(st.exited);
        assert_eq!(st.inheritor, inheritor_new);
        assert_eq!(st.messages, vec![m1, m2]);
        assert_eq!(st.value_to_receive, 3);
    }

    #[test]
    #[should_panic(expected = "squashed transition value overflow")]
    fn test_squash_value_overflow_panics() {
        let actor = ActorId::from([5; 32]);

        let _ = squash_transitions_by_actor(vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 42,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: u128::MAX - 10,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
        ]);
    }

    #[test]
    fn test_squash_preserves_distinct_actors() {
        let actor_a = ActorId::from([1; 32]);
        let actor_b = ActorId::from([2; 32]);

        let transitions = vec![
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([10; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 5,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor_b,
                new_state_hash: H256::from([20; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 10,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
        ];

        let squashed = squash_transitions_by_actor(transitions);
        assert_eq!(squashed.len(), 2);

        let st_a = squashed.iter().find(|t| t.actor_id == actor_a).unwrap();
        assert_eq!(st_a.new_state_hash, H256::from([10; 32]));
        assert_eq!(st_a.value_to_receive, 5);

        let st_b = squashed.iter().find(|t| t.actor_id == actor_b).unwrap();
        assert_eq!(st_b.new_state_hash, H256::from([20; 32]));
        assert_eq!(st_b.value_to_receive, 10);
    }

    #[test]
    fn test_squash_no_exit_preserves_inheritor_zero() {
        let actor = ActorId::from([3; 32]);

        let transitions = vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 1,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 2,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
        ];

        let squashed = squash_transitions_by_actor(transitions);
        assert_eq!(squashed.len(), 1);
        assert!(!squashed[0].exited);
        assert_eq!(squashed[0].inheritor, ActorId::zero());
        assert_eq!(squashed[0].value_to_receive, 3);
    }

    #[test]
    fn test_squash_comprehensive() {
        use ethexe_common::gear::{Message, ValueClaim};
        use gprimitives::MessageId;

        // --- Actors ---
        let actor_a = ActorId::from([0xAA; 32]); // appears in 3 blocks
        let actor_b = ActorId::from([0xBB; 32]); // appears in 2 blocks; later non-exit is defensive
        let actor_c = ActorId::from([0xCC; 32]); // appears only once (singleton)

        let inheritor_1 = ActorId::from([0x11; 32]);

        // --- Messages ---
        let msg = |tag: &[u8], val: u128| Message {
            id: MessageId::from(H256::from_slice(&{
                let mut buf = [0u8; 32];
                buf[..tag.len().min(32)].copy_from_slice(&tag[..tag.len().min(32)]);
                buf
            })),
            destination: ActorId::from([0xDD; 32]),
            payload: tag.to_vec(),
            value: val,
            reply_details: None,
            call: false,
        };
        let m_a1 = msg(b"a1", 10);
        let m_a2 = msg(b"a2", 20);
        let m_a3 = msg(b"a3", 30);
        let m_b1 = msg(b"b1", 100);
        let m_b2 = msg(b"b2", 200);
        let m_c1 = msg(b"c1", 50);

        // --- Value claims ---
        let vc = |id_byte: u8, val: u128| ValueClaim {
            message_id: MessageId::from(H256::from([id_byte; 32])),
            destination: ActorId::from([id_byte; 32]),
            value: val,
        };
        let vc_a1 = vc(0x01, 5);
        let vc_a2 = vc(0x02, 15);
        let vc_b1 = vc(0x03, 7);

        // Simulate transitions in chronological order (oldest first):
        //
        // Block 1: actor_a (state=H1, exit to inheritor_1, value=100, msg=a1, vc=vc_a1)
        //          actor_b (state=H3, exited=true inheritor_1, value=50, msg=b1, vc=vc_b1)
        // Block 2: actor_a (state=H2, no exit, value=200, msg=a2, vc=vc_a2)
        //          actor_b (state=H4, exited=false, value=25, msg=b2)
        // Block 3: actor_a (state=H_final, no exit, value=150, msg=a3, neg_sign=true)
        //          actor_c (state=H5, no exit, value=1, msg=c1) -- singleton
        let transitions = vec![
            // Block 1
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([0x01; 32]),
                exited: true,
                inheritor: inheritor_1,
                value_to_receive: 100,
                value_to_receive_negative_sign: false,
                value_claims: vec![vc_a1.clone()],
                messages: vec![m_a1.clone()],
            },
            StateTransition {
                actor_id: actor_b,
                new_state_hash: H256::from([0x03; 32]),
                exited: true,
                inheritor: inheritor_1,
                value_to_receive: 50,
                value_to_receive_negative_sign: false,
                value_claims: vec![vc_b1.clone()],
                messages: vec![m_b1.clone()],
            },
            // Block 2
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([0x02; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 200,
                value_to_receive_negative_sign: false,
                value_claims: vec![vc_a2.clone()],
                messages: vec![m_a2.clone()],
            },
            StateTransition {
                actor_id: actor_b,
                new_state_hash: H256::from([0x04; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 25,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m_b2.clone()],
            },
            // Block 3
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([0xFF; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 150,
                value_to_receive_negative_sign: true,
                value_claims: vec![],
                messages: vec![m_a3.clone()],
            },
            StateTransition {
                actor_id: actor_c,
                new_state_hash: H256::from([0x05; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 1,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![m_c1.clone()],
            },
        ];

        let squashed = squash_transitions_by_actor(transitions);

        // We look up each actor explicitly to keep assertions independent from
        // the sign-based output ordering.
        assert_eq!(squashed.len(), 3, "3 distinct actors expected");

        // --- actor_a: 3 transitions squashed ---
        let st_a = squashed.iter().find(|t| t.actor_id == actor_a).unwrap();
        // Newest state hash (block 3)
        assert_eq!(st_a.new_state_hash, H256::from([0xFF; 32]));
        // Block 1 exited, but blocks 2 & 3 did not—however once exited the flag sticks
        // only if any transition set exited=true. Here block 1 did, so exit_inheritor = inheritor_1
        // but then block 2 did not exit (no override) and block 3 did not exit (no override).
        // The latest exit was block 1 with inheritor_1.
        assert!(st_a.exited);
        assert_eq!(st_a.inheritor, inheritor_1);
        // Messages in chronological order: a1, a2, a3
        assert_eq!(st_a.messages, vec![m_a1, m_a2, m_a3]);
        // Value claims accumulated: vc_a1, vc_a2
        assert_eq!(st_a.value_claims, vec![vc_a1, vc_a2]);
        // value_to_receive: 100 + 200 - 150 = 150
        assert_eq!(st_a.value_to_receive, 150);
        assert!(!st_a.value_to_receive_negative_sign);

        // --- actor_b: 2 transitions squashed ---
        let st_b = squashed.iter().find(|t| t.actor_id == actor_b).unwrap();
        // Newest state hash (block 2)
        assert_eq!(st_b.new_state_hash, H256::from([0x04; 32]));
        // Block 1 exited with inheritor_1; block 2 does not exit. That second
        // transition is defensive coverage for an otherwise unreachable state,
        // so the latest exited transition is still block 1.
        assert!(st_b.exited);
        assert_eq!(st_b.inheritor, inheritor_1);
        // Messages: b1, b2
        assert_eq!(st_b.messages, vec![m_b1, m_b2]);
        // Value claims: only vc_b1
        assert_eq!(st_b.value_claims, vec![vc_b1]);
        // value: 50 + 25 = 75
        assert_eq!(st_b.value_to_receive, 75);
        assert!(!st_b.value_to_receive_negative_sign);

        // --- actor_c: singleton, passes through unchanged ---
        let st_c = squashed.iter().find(|t| t.actor_id == actor_c).unwrap();
        assert_eq!(st_c.new_state_hash, H256::from([0x05; 32]));
        assert!(!st_c.exited);
        assert_eq!(st_c.inheritor, ActorId::zero());
        assert_eq!(st_c.messages, vec![m_c1]);
        assert!(st_c.value_claims.is_empty());
        assert_eq!(st_c.value_to_receive, 1);
        assert!(!st_c.value_to_receive_negative_sign);
    }

    /// Exit in a later block overrides an earlier exit's inheritor.
    #[test]
    fn test_squash_later_exit_overrides_earlier() {
        let actor = ActorId::from([0xEE; 32]);
        let inheritor_early = ActorId::from([0x11; 32]);
        let inheritor_late = ActorId::from([0x22; 32]);

        let transitions = vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: true,
                inheritor: inheritor_early,
                value_to_receive: 0,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: true,
                inheritor: inheritor_late,
                value_to_receive: 0,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
        ];

        let squashed = squash_transitions_by_actor(transitions);
        assert_eq!(squashed.len(), 1);
        assert!(squashed[0].exited);
        assert_eq!(
            squashed[0].inheritor, inheritor_late,
            "latest exit's inheritor must win"
        );
    }

    #[test]
    fn test_squash_mixed_sign_value_to_receive() {
        let actor = ActorId::from([0xAB; 32]);

        let squashed = squash_transitions_by_actor(vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 100,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 50,
                value_to_receive_negative_sign: true,
                value_claims: vec![],
                messages: vec![],
            },
        ]);

        assert_eq!(squashed.len(), 1);
        assert_eq!(squashed[0].value_to_receive, 50);
        assert!(!squashed[0].value_to_receive_negative_sign);
    }

    #[test]
    fn test_squash_exact_value_cancellation() {
        let actor = ActorId::from([0xAC; 32]);

        let squashed = squash_transitions_by_actor(vec![
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 100,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 100,
                value_to_receive_negative_sign: true,
                value_claims: vec![],
                messages: vec![],
            },
        ]);

        assert_eq!(squashed.len(), 1);
        assert_eq!(squashed[0].value_to_receive, 0);
        assert!(!squashed[0].value_to_receive_negative_sign);
    }
}
