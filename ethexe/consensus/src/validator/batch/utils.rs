// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::validator::batch::{filler::BatchFiller, types::BatchParts};

use super::types::CodeNotValidatedError;

use anyhow::{Result, anyhow, bail};
use core::num::NonZero;
use ethexe_common::{
    SimpleBlockData,
    db::{BlockMetaStorageRO, CodesStorageRO, MbStorageRO, OnChainStorageRO},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition, ValueClaim,
    },
};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::{HashMap, hash_map::Entry};

/// MBs in `(last_committed_mb, mb_hash]`, chronological order. Strict: errors
/// if the walk doesn't reach the anchor or any MB along the way is not computed.
/// Used on the participant path; lenient producer counterpart is
/// [`collect_computed_uncommitted_predecessors`].
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
        current = db
            .mb_compact_block(current)
            .ok_or_else(|| anyhow!("MB {current} missing compact block — DB invariant"))?
            .parent;
    }

    Ok(mbs.into_iter().rev().collect())
}

/// Producer-path lenient counterpart: longest computed prefix anchored at
/// `last_committed_mb`. Returns empty when the first successor isn't yet
/// computed or the parent walk doesn't reach the anchor (e.g. fresh restart).
pub fn collect_computed_uncommitted_predecessors<DB: MbStorageRO>(
    db: &DB,
    last_committed_mb: H256,
    mb_head: H256,
) -> Vec<H256> {
    // Walk the parent chain backward from `mb_head` until we either
    // reach `last_committed_mb` or run off the local chain.
    let mut chain = Vec::new(); // newest-first
    let mut current = mb_head;
    while current != last_committed_mb && current != H256::zero() {
        let meta = db.mb_meta(current);
        chain.push((current, meta.computed));
        current = db
            .mb_compact_block(current)
            .map(|c| c.parent)
            .unwrap_or(H256::zero());
    }
    if current != last_committed_mb {
        // Walk didn't reach the anchor (fast-restart / sync-lag); caller retries.
        tracing::warn!(
            %last_committed_mb,
            %mb_head,
            walk_depth = chain.len(),
            "parent walk did not reach last_committed_mb — chain commitment skipped",
        );
        return Vec::new();
    }

    chain.reverse();

    // Longest contiguous computed prefix anchored at `last_committed_mb`.
    let mut collected = Vec::with_capacity(chain.len());
    for (hash, computed) in chain.iter().copied() {
        if !computed {
            break;
        }
        collected.push(hash);
    }
    collected
}

/// `true` iff `candidate` is reachable from `latest_finalized_mb` by walking
/// `parent_mb_hash`. Sound by BFT linear-order; bounded by the height gap.
/// `H256::zero()` is the genesis sentinel.
pub fn is_finalized_locally<DB: MbStorageRO>(
    db: &DB,
    candidate: H256,
    latest_finalized_mb: H256,
) -> bool {
    if candidate == H256::zero() || candidate == latest_finalized_mb {
        return true;
    }
    if latest_finalized_mb == H256::zero() {
        return false;
    }
    let mut current = latest_finalized_mb;
    while current != H256::zero() {
        if current == candidate {
            return true;
        }
        current = db
            .mb_compact_block(current)
            .map(|c| c.parent)
            .unwrap_or(H256::zero());
    }
    false
}

pub fn create_batch_commitment<DB: BlockMetaStorageRO>(
    db: &DB,
    block: &SimpleBlockData,
    batch_parts: BatchParts,
    commitment_delay_limit: std::num::NonZero<u8>,
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
        tracing::debug!("No commitments for block {block_hash} - skip batch commitment");
        return Ok(None);
    }

    let previous_batch = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(
            || anyhow!("Cannot get from db last committed block for block {block_hash}",),
        )?;

    let expiry: u8 = commitment_delay_limit.get();

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

/// Producer chain-commitment builder: covers `(last_committed_mb..mb_head]` up
/// to where compute has reached, fits within the size budget, returns the
/// head MB actually included. Returns `last_committed_mb` if nothing fits.
pub fn try_include_chain_commitment<DB: BlockMetaStorageRO + MbStorageRO>(
    db: &DB,
    at_block: H256,
    mb_head: H256,
    batch_filler: &mut BatchFiller,
) -> Result<H256> {
    let last_committed_mb = db
        .block_meta(at_block)
        .last_committed_mb
        .unwrap_or(H256::zero());

    let pending = collect_computed_uncommitted_predecessors(db, last_committed_mb, mb_head);

    if pending.is_empty() {
        // Nothing computed in range; producer skips chain commitment this round.
        return Ok(last_committed_mb);
    }

    // Aggregate transitions incrementally; stop when the next MB blows the size budget.
    let mut transitions: Vec<StateTransition> = Vec::new();
    let mut last_included = last_committed_mb;
    for mb_hash in &pending {
        let Some(mb_transitions) = db.mb_outcome(*mb_hash) else {
            anyhow::bail!("Computed MB {mb_hash} outcome not found in db");
        };

        // Trial-fit this MB; bail if it pushes us past the batch size budget.
        let len_before = transitions.len();
        transitions.extend(mb_transitions);
        let trial_commitment = ChainCommitment {
            head: *mb_hash,
            transitions,
            last_advanced_eth_block: db.mb_meta(*mb_hash).last_advanced_eb,
        };
        let would_fit = batch_filler.would_fit_chain_commitment(&trial_commitment);
        transitions = trial_commitment.transitions;

        if !would_fit {
            let _ = transitions.split_off(len_before);
            break;
        }

        last_included = *mb_hash;
    }

    // Skip the commitment entirely when there are no state transitions
    // to carry on-chain. Pushing the Ethereum anchor forward on every
    // idle round would spam pointless batches; the dedicated checkpoint
    // path ([`try_include_checkpoint_chain_commitment`]) gates that on
    // `uncommitted_chain_len_threshold` and emits the empty-transitions
    // commitment only after a long quiet stretch.
    if transitions.is_empty() {
        return Ok(last_committed_mb);
    }

    let commitment = ChainCommitment {
        head: last_included,
        transitions,
        last_advanced_eth_block: db.mb_meta(last_included).last_advanced_eb,
    };

    if let Err(err) = batch_filler.include_chain_commitment(commitment) {
        tracing::trace!(
            "failed to include chain commitment for head MB {mb_head} because of error={err}"
        );
        return Ok(last_committed_mb);
    }

    Ok(last_included)
}

/// If `last_advanced_eth_block` of `mb_head` is more than `threshold` Eth blocks
/// past `block.last_committed_eb`, force an empty chain commitment
/// that pins the head MB and the new advanced anchor on-chain.
pub fn try_include_checkpoint_chain_commitment<
    DB: BlockMetaStorageRO + MbStorageRO + OnChainStorageRO,
>(
    db: &DB,
    at_block: H256,
    mb_head: H256,
    threshold: NonZero<u32>,
    batch_filler: &mut BatchFiller,
) -> Result<()> {
    let advanced = db.mb_meta(mb_head).last_advanced_eb;
    if advanced.is_zero() {
        return Ok(());
    }
    let Some(advanced_header) = db.block_header(advanced) else {
        return Ok(());
    };

    // `at_block` is `prepared` by the time the coordinator runs (see
    // `Idle`), so the field must be populated.
    let last_committed_advanced = db.block_meta(at_block).last_committed_eb.ok_or_else(|| {
        anyhow::anyhow!("block_meta({at_block}).last_committed_eb missing despite prepared==true")
    })?;
    let last_committed_height = if last_committed_advanced.is_zero() {
        0
    } else {
        db.block_header(last_committed_advanced)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "block_header({last_committed_advanced}) missing for at_block {at_block}"
                )
            })?
            .height
    };

    let gap = advanced_header.height.saturating_sub(last_committed_height);
    if gap <= threshold.get() {
        return Ok(());
    }

    let commitment = ChainCommitment {
        head: mb_head,
        transitions: Vec::new(),
        last_advanced_eth_block: advanced,
    };

    if let Err(err) = batch_filler.include_chain_commitment(commitment) {
        tracing::trace!(
            "checkpoint chain commitment didn't fit (head {mb_head}, advanced {advanced}): {err}"
        );
    } else {
        tracing::info!(
            %mb_head,
            %advanced,
            gap,
            threshold = threshold.get(),
            "emitting checkpoint chain commitment"
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Schedule,
        db::{CompactMb, MbStorageRW},
        malachite::{ProcessQueuesLimits, Transaction, Transactions},
    };
    use ethexe_db::Database;

    /// Per-height unique CAS via `AdvanceTillEthereumBlock` salt.
    fn empty_txs(height: u64) -> Transactions {
        Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: H256::from_low_u64_be(0xEB00 + height),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ])
    }

    /// Mimics malachite `process_mb_proposal` + executor's `meta.computed` flip.
    fn write_mb(
        db: &Database,
        parent_mb: H256,
        height: u64,
        outcome: Vec<StateTransition>,
    ) -> H256 {
        let txs = empty_txs(height);
        let transactions_hash = db.set_transactions(txs);
        // Synthetic mb_hash; only uniqueness matters here.
        let mb_hash = H256::from_low_u64_be(0x1000 + height);
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: parent_mb,
                height,
                transactions_hash,
            },
        );
        db.set_mb_outcome(mb_hash, outcome);
        db.set_mb_schedule(mb_hash, Schedule::default());
        db.mutate_mb_meta(mb_hash, |meta| {
            meta.computed = true;
            meta.last_advanced_eb = H256::zero();
        });
        mb_hash
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
    fn lenient_collect_returns_full_range_when_all_computed() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);

        let walked = collect_computed_uncommitted_predecessors(&db, H256::zero(), mb3);
        assert_eq!(walked, vec![mb1, mb2, mb3]);

        let from_mb1 = collect_computed_uncommitted_predecessors(&db, mb1, mb3);
        assert_eq!(from_mb1, vec![mb2, mb3]);
    }

    #[test]
    fn lenient_collect_truncates_at_first_uncomputed() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);
        // Compute is lagging: mb2 hasn't finished yet.
        db.mutate_mb_meta(mb2, |meta| meta.computed = false);

        // Only mb1 is contiguous-computed from anchor; mb2 gap blocks the rest.
        let walked = collect_computed_uncommitted_predecessors(&db, H256::zero(), mb3);
        assert_eq!(walked, vec![mb1]);
    }

    #[test]
    fn lenient_collect_returns_empty_when_first_successor_uncomputed() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        db.mutate_mb_meta(mb1, |meta| meta.computed = false);

        let walked = collect_computed_uncommitted_predecessors(&db, H256::zero(), mb1);
        assert!(walked.is_empty());
    }

    #[test]
    fn lenient_collect_returns_empty_when_chain_does_not_reach_anchor() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);

        let bogus = H256::from_low_u64_be(0xDEAD);
        // Walk doesn't hit `bogus`; producer skips silently instead of erroring.
        let walked = collect_computed_uncommitted_predecessors(&db, bogus, mb1);
        assert!(walked.is_empty());
    }

    #[test]
    fn lenient_collect_returns_empty_when_at_target() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);

        let walked = collect_computed_uncommitted_predecessors(&db, mb1, mb1);
        assert!(walked.is_empty());
    }

    #[test]
    fn is_finalized_zero_candidate_is_universally_finalized() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        assert!(is_finalized_locally(&db, H256::zero(), mb1));
        // Even with no local finalization yet, zero is the genesis sentinel.
        assert!(is_finalized_locally(&db, H256::zero(), H256::zero()));
    }

    #[test]
    fn is_finalized_self_is_finalized() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        assert!(is_finalized_locally(&db, mb1, mb1));
    }

    #[test]
    fn is_finalized_resolves_proper_ancestor_of_finalized_head() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);
        // Latest finalized is mb3 → mb1 and mb2 are also finalized.
        assert!(is_finalized_locally(&db, mb1, mb3));
        assert!(is_finalized_locally(&db, mb2, mb3));
    }

    #[test]
    fn is_finalized_returns_false_for_descendant_of_finalized_head() {
        // Speculative-but-not-yet-finalized candidate must fail strict check.
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        let mb2 = write_mb(&db, mb1, 2, vec![]);
        let mb3 = write_mb(&db, mb2, 3, vec![]);
        assert!(!is_finalized_locally(&db, mb3, mb1));
        assert!(!is_finalized_locally(&db, mb2, mb1));
    }

    #[test]
    fn is_finalized_returns_false_when_no_local_finalization() {
        let db = Database::memory();
        let mb1 = write_mb(&db, H256::zero(), 1, vec![]);
        assert!(!is_finalized_locally(&db, mb1, H256::zero()));
    }

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
            chain_commitment: Some(ChainCommitment {
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
            }),
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
            )
            .unwrap()
            .expect("non-empty batch commitment");
            assert_eq!(commitment.expiry, raw_limit);
            assert_eq!(commitment.previous_batch, last_committed_batch);
            assert_eq!(commitment.block_hash, block_hash);
        }
    }

    #[test]
    fn is_finalized_returns_false_on_disjoint_chain() {
        let db = Database::memory();
        let chain_a = write_mb(&db, H256::zero(), 1, vec![]);
        let chain_b_root = H256::from_low_u64_be(0xB001);
        db.set_mb_compact_block(
            chain_b_root,
            CompactMb {
                parent: H256::from_low_u64_be(0xB000), // unknown parent
                height: 1,
                transactions_hash: db.set_transactions(empty_txs(99)),
            },
        );
        assert!(!is_finalized_locally(&db, chain_b_root, chain_a));
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
