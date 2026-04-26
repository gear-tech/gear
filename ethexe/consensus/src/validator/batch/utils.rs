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
    SimpleBlockData,
    db::{BlockMetaStorageRO, CodesStorageRO, MbStorageRO},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition, ValueClaim,
    },
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{HashMap, hash_map::Entry};

/// Walk the MB chain from `mb_hash` up via `parent_mb_hash` and return the
/// hashes of all MBs strictly between `last_committed_mb` (exclusive) and
/// `mb_hash` (inclusive), in **chronological** order (oldest first).
///
/// `last_committed_mb == H256::zero()` means nothing has been committed
/// on-chain yet — the walk continues through the genesis MB and stops when
/// `parent_mb_hash` is `None`.
///
/// Errors out if the chain walk is exhausted without reaching
/// `last_committed_mb` (i.e. the supplied head is not a descendant of the
/// last committed MB), or if any MB along the way is not yet computed.
pub fn collect_not_committed_mb_predecessors<DB: MbStorageRO>(
    db: &DB,
    last_committed_mb: H256,
    mb_hash: H256,
) -> Result<Vec<H256>> {
    let mut mbs = Vec::new();
    let mut current = mb_hash;

    while current != last_committed_mb {
        if current == H256::zero() {
            bail!(
                "MB chain walk reached genesis without finding last_committed_mb {last_committed_mb}"
            );
        }

        let meta = db.mb_meta(current);
        if !meta.computed {
            bail!("MB {current} in chain is not computed");
        }

        mbs.push(current);
        current = meta.parent_mb_hash.unwrap_or(H256::zero());
    }

    Ok(mbs.into_iter().rev().collect())
}

/// `request.head` must be either `latest_finalized_mb` itself or one of its
/// ancestors (via `parent_mb_hash`). Returns `Ok(true)` when the candidate is
/// a non-strict ancestor of (or equal to) `latest_finalized_mb`. Returns
/// `Ok(false)` when the chain walk exhausts without hitting the candidate.
///
/// `H256::zero()` is treated as the pre-genesis sentinel and is an ancestor
/// of every MB.
pub fn is_ancestor_or_equal<DB: MbStorageRO>(
    db: &DB,
    candidate: H256,
    latest_finalized_mb: H256,
) -> Result<bool> {
    if candidate == H256::zero() {
        return Ok(true);
    }
    let mut current = latest_finalized_mb;
    while current != H256::zero() {
        if current == candidate {
            return Ok(true);
        }
        current = db.mb_meta(current).parent_mb_hash.unwrap_or(H256::zero());
    }
    Ok(false)
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO>(
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

    // An MB that finalized with no state transitions doesn't justify a chain
    // commitment on its own — head can advance the next time the coordinator
    // catches a non-empty MB. We still allow a batch *without* a chain
    // commitment if there are codes / validators / rewards to commit.
    let chain_has_transitions = chain_commitment
        .as_ref()
        .is_some_and(|c| !c.transitions.is_empty());

    if !chain_has_transitions
        && code_commitments.is_empty()
        && validators_commitment.is_none()
        && rewards_commitment.is_none()
    {
        tracing::debug!("No commitments for block {block_hash} - skip batch commitment");
        return Ok(None);
    }

    // Drop the chain commitment if its transitions list is empty — see comment above.
    let chain_commitment = chain_commitment.filter(|c| !c.transitions.is_empty());

    let previous_batch = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(
            || anyhow!("Cannot get from db last committed block for block {block_hash}",),
        )?;

    // For now we use a static expiry derived from `commitment_delay_limit` —
    // batches need to land within that many Ethereum blocks of `block.hash`
    // or they're rejected on-chain. Fine-grained expiry (depending on chain
    // deepness) is dropped along with the producer-side announce flow.
    let expiry: u8 = commitment_delay_limit
        .try_into()
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

/// Build a chain commitment that covers all not-yet-committed MBs between
/// `block.last_committed_mb` (exclusive) and `mb_head` (inclusive), feed it
/// into the supplied `batch_filler`, and return the hash of the head MB
/// that was actually included (might be older than `mb_head` if the size
/// budget was hit mid-walk; might equal `last_committed_mb` if the head MB's
/// outcome is empty and the filler skips the whole chain commitment).
pub fn try_include_chain_commitment<DB: BlockMetaStorageRO + MbStorageRO>(
    db: &DB,
    at_block: H256,
    mb_head: H256,
    batch_filler: &mut BatchFiller,
) -> Result<H256> {
    if !db.mb_meta(mb_head).computed {
        anyhow::bail!("Head MB {mb_head} is not computed, cannot aggregate chain commitment");
    }

    let last_committed_mb = db
        .block_meta(at_block)
        .last_committed_mb
        .unwrap_or(H256::zero());

    let pending = collect_not_committed_mb_predecessors(db, last_committed_mb, mb_head)?;

    // Aggregate transitions across the whole pending range; head advances to
    // the actual head MB even if intermediate MBs had empty outcomes (we
    // still want to advance the on-chain pointer past them next commit).
    let mut transitions = Vec::new();
    let mut last_included = last_committed_mb;
    for mb_hash in &pending {
        let Some(mb_transitions) = db.mb_outcome(*mb_hash) else {
            anyhow::bail!("Computed MB {mb_hash} outcome not found in db");
        };
        transitions.extend(mb_transitions);
        last_included = *mb_hash;
    }

    let commitment = ChainCommitment {
        head: last_included,
        transitions,
    };

    if let Err(err) = batch_filler.include_chain_commitment(commitment) {
        tracing::trace!(
            "failed to include chain commitment for head MB {mb_head} because of error={err}"
        );
        // include_chain_commitment only fails on size budget; report the head
        // we tried to commit so the caller can record what didn't fit.
        return Ok(last_committed_mb);
    }

    Ok(last_included)
}

/// Squashes transitions for the same actor into a single transition per actor.
///
/// For each actor, the newest transition (last in chronological order) provides the
/// `new_state_hash`. Messages, value claims, and `value_to_receive` are accumulated
/// from all transitions. If any transition marks the actor as exited, the resulting
/// inheritor is taken from the newest exit transition. The returned transitions
/// preserve the order in which each actor first appeared; callers apply any
/// later ordering required for commitment encoding or execution.
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
                aggregations[*entry.get()].join(transition);
            }
        }
    }

    aggregations
        .into_iter()
        .map(|aggregation| aggregation.finish())
        .collect()
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

    fn join(&mut self, mut transition: StateTransition) {
        let actor_id = transition.actor_id;
        debug_assert_eq!(self.newest.actor_id, actor_id);
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
    use ethexe_common::{
        Schedule,
        db::MbStorageRW,
        mb::{ProcessQueuesLimits, SequencerBlock, Transaction},
    };
    use ethexe_db::Database;

    fn empty_mb(parent: H256) -> SequencerBlock {
        SequencerBlock::new(
            parent,
            vec![Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            }],
        )
    }

    fn write_mb(
        db: &Database,
        parent_mb: H256,
        height: u64,
        outcome: Vec<StateTransition>,
    ) -> H256 {
        let block = empty_mb(parent_mb);
        let hash = block.hash();
        db.set_mb_block(hash, block);
        db.set_mb_outcome(hash, outcome);
        db.set_mb_schedule(hash, Schedule::default());
        db.mutate_mb_meta(hash, |meta| {
            meta.computed = true;
            meta.height = height;
            meta.parent_mb_hash = (parent_mb != H256::zero()).then_some(parent_mb);
            meta.last_advanced_block = H256::zero();
        });
        db.set_mb_hash_at_height(height, hash);
        hash
    }

    #[test]
    fn collect_predecessors_walks_chain() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);

        let walked = collect_not_committed_mb_predecessors(&db, H256::zero(), mb3).unwrap();
        assert_eq!(walked, vec![mb1, mb2, mb3]);

        let from_mb1 = collect_not_committed_mb_predecessors(&db, mb1, mb3).unwrap();
        assert_eq!(from_mb1, vec![mb2, mb3]);
    }

    #[test]
    fn collect_predecessors_returns_empty_when_at_target() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);

        let walked = collect_not_committed_mb_predecessors(&db, mb1, mb1).unwrap();
        assert!(walked.is_empty());
    }

    #[test]
    fn collect_predecessors_errors_when_target_not_in_chain() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);

        // mb2 cannot trace back to a hash that's not on the chain.
        let bogus = H256::from_low_u64_be(0xDEAD);
        let err = collect_not_committed_mb_predecessors(&db, bogus, mb2).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("genesis"), "got: {msg}");
    }

    #[test]
    fn collect_predecessors_errors_on_uncomputed_mb() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        // Force mb2 to look uncomputed.
        db.mutate_mb_meta(mb2, |meta| meta.computed = false);

        let err = collect_not_committed_mb_predecessors(&db, H256::zero(), mb2).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not computed"), "got: {msg}");
    }

    #[test]
    fn is_ancestor_zero_is_universal_ancestor() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        assert!(is_ancestor_or_equal(&db, H256::zero(), mb1).unwrap());
    }

    #[test]
    fn is_ancestor_self_is_ancestor() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        assert!(is_ancestor_or_equal(&db, mb1, mb1).unwrap());
    }

    #[test]
    fn is_ancestor_resolves_proper_ancestor() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);
        assert!(is_ancestor_or_equal(&db, mb1, mb3).unwrap());
        // Going the other way around is not an ancestor relationship.
        assert!(!is_ancestor_or_equal(&db, mb3, mb1).unwrap());
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
    fn test_squash_preserves_first_seen_actor_order() {
        let actor_a = ActorId::from([0xA1; 32]);
        let actor_b = ActorId::from([0xB2; 32]);

        let squashed = squash_transitions_by_actor(vec![
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([1; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 10,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor_b,
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 5,
                value_to_receive_negative_sign: true,
                value_claims: vec![],
                messages: vec![],
            },
            StateTransition {
                actor_id: actor_a,
                new_state_hash: H256::from([3; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 1,
                value_to_receive_negative_sign: false,
                value_claims: vec![],
                messages: vec![],
            },
        ]);

        assert_eq!(
            squashed
                .iter()
                .map(|transition| transition.actor_id)
                .collect::<Vec<_>>(),
            vec![actor_a, actor_b]
        );
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
