// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::validator::batch::{filler::BatchFiller, types::BatchParts};

use anyhow::{Context, Result, anyhow};
use ethexe_common::{
    BlockHeader, SimpleBlockData,
    db::{
        BlockMetaStorageRO, CodesStorageRO, ConfigStorageRO, GlobalsStorageRO, MbStorageRO,
        OnChainStorageRO,
    },
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition, ValueClaim,
    },
};
use gprimitives::{ActorId, H256};
use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    num::NonZero,
};

pub fn create_batch_commitment<DB: BlockMetaStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    batch_parts: BatchParts,
    commitment_delay_limit: NonZero<u8>,
    checkpoint_threshold: NonZero<u32>,
) -> Result<Option<BatchCommitment>> {
    let BatchParts {
        chain_commitment: chain_commitment_with_len,
        validators_commitment,
        code_commitments,
        rewards_commitment,
    } = batch_parts;

    let SimpleBlockData {
        hash: block_hash,
        header: BlockHeader { timestamp, .. },
    } = *block;

    let has_other_commitments = !code_commitments.is_empty()
        || validators_commitment.is_some()
        || rewards_commitment.is_some();

    let chain_commitment = match chain_commitment_with_len {
        Some((commitment, len)) => {
            // A chain commitment carrying no transitions only earns its place
            // when it advances the on-chain Ethereum anchor after a long quiet
            // stretch (checkpoint threshold reached) or when the batch already
            // carries other commitments.
            if !has_other_commitments
                && commitment.transitions.is_empty()
                && len < checkpoint_threshold
            {
                tracing::debug!(
                    %block_hash,
                    %len,
                    %checkpoint_threshold,
                    "Chain commitment is empty and checkpoint threshold not reached, skip batch commitment"
                );
                return Ok(None);
            }

            tracing::debug!(
                %block_hash,
                %len,
                %checkpoint_threshold,
                transitions_len = commitment.transitions.len(),
                "Including chain commitment into batch"
            );
            Some(commitment)
        }
        None => {
            if !has_other_commitments {
                tracing::debug!(%block_hash, "Nothing to commit, skip batch commitment");
                return Ok(None);
            }
            None
        }
    };

    let previous_batch = db
        .block_meta(block_hash)
        .last_committed_batch
        .with_context(|| {
            format!("Cannot get from db last committed block for block {block_hash}")
        })?;

    Ok(Some(BatchCommitment {
        block_hash,
        timestamp,
        previous_batch,
        expiry: commitment_delay_limit.get(),
        chain_commitment,
        code_commitments,
        validators_commitment,
        rewards_commitment,
    }))
}

/// Producer-side helper: take the block's `codes_queue`, aggregate validated
/// code commitments, and push them into the batch filler in queue order. Stops
/// once the filler rejects further additions (e.g. size limit).
pub fn aggregate_code_commitments_for_block<DB: CodesStorageRO + BlockMetaStorageRO>(
    db: &DB,
    block_hash: H256,
    batch_filler: &mut BatchFiller,
) -> Result<()> {
    let queue = db
        .block_meta(block_hash)
        .codes_queue
        .ok_or_else(|| anyhow!("Computed block {block_hash} codes queue is not in storage"))?;

    for commitment in queue
        .into_iter()
        .filter_map(|id| db.code_valid(id).map(|valid| CodeCommitment { id, valid }))
    {
        if let Err(err) = batch_filler.include_code_commitment(commitment) {
            tracing::trace!(
                "filler rejects code commitment: {err}, stop including more code commitments"
            );
            break;
        }
    }

    Ok(())
}

/// Producer chain-commitment builder.
pub fn try_include_chain_commitment<
    DB: ConfigStorageRO + GlobalsStorageRO + BlockMetaStorageRO + MbStorageRO + OnChainStorageRO,
>(
    db: &DB,
    at_block: H256,
    batch_filler: &mut BatchFiller,
) -> Result<()> {
    let latest_finalized_mb = db.globals().latest_finalized_mb_hash;
    if latest_finalized_mb.is_zero() {
        return Ok(());
    }

    let latest_advanced_eb_hash = db
        .mb_meta(latest_finalized_mb)
        .last_advanced_eb
        .context("latest finalized mb must have latest advanced eb info")?;

    if !is_strict_descendant_eth_block(db, at_block, latest_advanced_eb_hash)? {
        tracing::error!(
            %at_block,
            %latest_finalized_mb,
            %latest_advanced_eb_hash,
            "latest advanced eth block is not strict ancestor of the current chain head, skipping chain commitment"
        );
        return Ok(());
    }

    let last_committed_mb_hash = db
        .block_meta(at_block)
        .last_committed_mb
        .with_context(|| format!("at_block {at_block} must be prepared at this moment"))?;

    let Some(last_committed_mb) = db.mb_compact_block(last_committed_mb_hash) else {
        tracing::warn!(
            %at_block,
            %latest_finalized_mb,
            %last_committed_mb_hash,
            "last committed MB {last_committed_mb_hash} is still not synced locally, skipping chain commitment"
        );
        return Ok(());
    };

    let mut cursor_mb_hash = latest_finalized_mb;
    let mut cursor_mb = db
        .mb_compact_block(cursor_mb_hash)
        .context("latest finalized MB must have compact block in db")?;

    // Reach the latest computed MB on the chain
    while !db.mb_meta(cursor_mb_hash).computed {
        if cursor_mb_hash == last_committed_mb_hash {
            tracing::debug!(
                %at_block,
                %latest_finalized_mb,
                %last_committed_mb_hash,
                "no computed MBs since latest committed MB, skipping chain commitment"
            );
            return Ok(());
        }
        if cursor_mb.height <= last_committed_mb.height {
            tracing::error!(
                %at_block,
                %latest_finalized_mb,
                %last_committed_mb_hash,
                "latest finalized MB and last committed MB are not in the same chain, protocol violation, skipping chain commitment"
            );
            return Ok(());
        }

        cursor_mb_hash = cursor_mb.parent;
        cursor_mb = db
            .mb_compact_block(cursor_mb_hash)
            .context("failed to fetch compact block for finalized MB")?;
    }

    // Reach the last last_committed_mb. Collect blocks on the way to build the commitment.
    let mut computed_not_committed_mbs = VecDeque::new();
    while cursor_mb_hash != last_committed_mb_hash {
        if cursor_mb.height <= last_committed_mb.height {
            tracing::error!(
                %at_block,
                %latest_finalized_mb,
                %last_committed_mb_hash,
                "latest finalized MB and last committed MB are not in the same chain, protocol violation, skipping chain commitment"
            );
            return Ok(());
        }

        // push_front to maintain chronological order from oldest to newest
        computed_not_committed_mbs.push_front(cursor_mb_hash);
        cursor_mb_hash = cursor_mb.parent;
        cursor_mb = db
            .mb_compact_block(cursor_mb_hash)
            .context("failed to fetch compact block for finalized MB")?;
    }

    // Collect commitment
    for cursor in computed_not_committed_mbs.into_iter() {
        let transitions = db
            .mb_outcome(cursor)
            .with_context(|| format!("computed MB {cursor} outcome not found in db"))?;

        let last_advanced_eth_block = db
            .mb_meta(cursor)
            .last_advanced_eb
            .with_context(|| format!("computed MB {cursor} has no last_advanced_eb in db"))?;

        let one_block_commitment = ChainCommitment {
            head: cursor,
            transitions,
            last_advanced_eth_block,
        };

        // Producer is lenient: once an MB would push the batch past the size
        // budget, stop here and commit what fits. The next round picks up the
        // remainder.
        if batch_filler
            .append_chain_commitment(one_block_commitment)
            .is_err()
        {
            tracing::debug!(
                %cursor,
                "chain commitment size limit reached, committing collected MBs only"
            );
            break;
        }
    }

    Ok(())
}

/// Collapse repeated actor transitions: newest `new_state_hash`, accumulated
/// messages / value claims / `value_to_receive`, exit-inheritor from the newest
/// exit. First-seen order is preserved.
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

/// `(u128, negative)` signed magnitude — addition for squashing transitions.
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
    // Invert key so router-returning transitions come before receiving ones.
    transitions.sort_by_key(|transition| !transition.value_to_receive_negative_sign);
}

pub fn has_duplicates<T: std::hash::Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

pub fn is_strict_descendant_eth_block<DB: OnChainStorageRO>(
    db: &DB,
    block: H256,
    ancestor: H256,
) -> Result<bool> {
    if ancestor.is_zero() {
        // The genesis/pre-genesis anchor is an ancestor-or-equal of every
        // anchor, including the genesis anchor itself: a chain commitment is
        // allowed even when the Eth anchor has not advanced past genesis yet.
        return Ok(true);
    }

    let ancestor_height = db
        .block_header(ancestor)
        .ok_or_else(|| anyhow!("eth chain walk: missing header for ancestor {ancestor}"))?
        .height;

    let mut current = block;
    while current != ancestor {
        if current.is_zero() {
            return Ok(false);
        }
        let header = db
            .block_header(current)
            .ok_or_else(|| anyhow!("eth chain walk: missing header for {current}"))?;
        if header.height <= ancestor_height {
            return Ok(false);
        }
        current = header.parent_hash;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_db::Database;

    #[test]
    fn create_batch_commitment_writes_commitment_delay_limit_into_expiry() {
        use ethexe_common::{
            BlockHeader, SimpleBlockData,
            db::{BlockMetaStorageRW, OnChainStorageRW},
            gear::{ChainCommitment, StateTransition},
        };
        use std::num::NonZero;

        let db = Database::memory();
        let block_hash = H256::from_low_u64_be(0xB10C);
        db.set_block_header(
            block_hash,
            BlockHeader {
                height: 7,
                parent_hash: H256::zero(),
                timestamp: 1234,
            },
        );
        let last_committed_batch = ethexe_common::Digest::random();
        db.mutate_block_meta(block_hash, |meta| {
            meta.last_committed_batch = Some(last_committed_batch);
        });
        let block = SimpleBlockData {
            hash: block_hash,
            header: db.block_header(block_hash).unwrap(),
        };

        let parts = BatchParts {
            chain_commitment: Some((
                ChainCommitment {
                    transitions: vec![StateTransition {
                        actor_id: gprimitives::ActorId::from([0xAB; 32]),
                        new_state_hash: H256::from_low_u64_be(0xDEAD_BEEF),
                        exited: false,
                        inheritor: Default::default(),
                        value_to_receive: 0,
                        value_to_receive_negative_sign: false,
                        value_claims: vec![],
                        messages: vec![],
                    }],
                    head: block_hash,
                    last_advanced_eth_block: H256::zero(),
                },
                NonZero::new(1).unwrap(),
            )),
            code_commitments: vec![],
            validators_commitment: None,
            rewards_commitment: None,
        };

        // Coordinator-local knob: expiry on the BatchCommitment must
        // exactly mirror `commitment_delay_limit.get()` from the
        // validator config so the on-chain submission path honors the
        // operator-configured delay.
        for raw_limit in [1u8, 3, 5, 32, u8::MAX] {
            let commitment = create_batch_commitment(
                &db,
                &block,
                parts.clone(),
                NonZero::new(raw_limit).unwrap(),
                NonZero::new(1).unwrap(),
            )
            .unwrap()
            .expect("non-empty batch commitment");
            assert_eq!(commitment.expiry, raw_limit);
            assert_eq!(commitment.previous_batch, last_committed_batch);
            assert_eq!(commitment.block_hash, block_hash);
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
