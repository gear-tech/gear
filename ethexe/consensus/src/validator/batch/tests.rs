// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Integration tests for [`BatchCommitmentManager`].
//!
//! The cases below exercise the end-to-end create→validate round-trip
//! over the MB-driven flow: a batch is built from a chain of finalized
//! MBs, a [`BatchCommitmentValidationRequest`] is derived, and the
//! manager re-derives the same batch independently and signs (or
//! rejects) it.

use super::{BatchCommitmentManager, BatchLimits, ValidationStatus, types::ValidationRejectReason};
use crate::validator::core::MiddlewareWrapper;
use ethexe_common::{
    Address, Digest, ProgramStates, Schedule, SimpleBlockData, ToDigest, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest,
    db::{BlockMetaStorageRW, CompactMb, GlobalsStorageRW, MbStorageRW, SetConfig},
    gear::StateTransition,
    malachite::{ProcessQueuesLimits, Transaction, Transactions},
    mock::*,
};
use ethexe_db::Database;
use ethexe_ethereum::middleware::{ElectionProvider, MockElectionProvider};
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{ActorId, CodeId, H256, U256};
use std::num::{NonZero, NonZeroU64};

const BLOCK_GAS_LIMIT: u64 = ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;

fn mock_batch_manager_with_limits(db: Database, limits: BatchLimits) -> BatchCommitmentManager {
    let (manager, _) = mock_batch_manager_with_limits_and_election(db, limits);
    manager
}

/// Variant of [`mock_batch_manager_with_limits`] that returns the
/// underlying [`MockElectionProvider`] handle so the caller can pre-load
/// canned election results before calling
/// [`BatchCommitmentManager::aggregate_validators_commitment`].
///
/// The handle is `Clone` and shares state with the one boxed into the
/// manager — both observe the same `predefined_election_at` map.
fn mock_batch_manager_with_limits_and_election(
    db: Database,
    limits: BatchLimits,
) -> (BatchCommitmentManager, MockElectionProvider) {
    let election = MockElectionProvider::new();
    let middleware =
        MiddlewareWrapper::from_inner(Box::new(election.clone()) as Box<dyn ElectionProvider>);
    (
        BatchCommitmentManager::new(limits, db, middleware),
        election,
    )
}

fn mock_batch_manager(db: Database) -> BatchCommitmentManager {
    mock_batch_manager_with_limits(db, BatchLimits::default())
}

/// Append a single MB to the chain. Sets the meta as `computed=true`
/// so the manager treats it as finalized state available for batching.
fn append_mb(db: &Database, parent: H256, height: u64, outcome: Vec<StateTransition>) -> H256 {
    let txs = Transactions::new(vec![
        Transaction::AdvanceTillEthereumBlock {
            block_hash: H256::from_low_u64_be(0xEB00 + height),
        },
        Transaction::ProcessQueues {
            limits: ProcessQueuesLimits::default(),
        },
    ]);
    let transactions_hash = db.set_transactions(txs);
    // Synthetic mb_hash — uniqueness is what matters here.
    let mb_hash = H256::from_low_u64_be(0x1000 + height);
    db.set_mb_compact_block(
        mb_hash,
        CompactMb {
            parent,
            height,
            transactions_hash,
        },
    );
    db.set_mb_outcome(mb_hash, outcome);
    db.set_mb_schedule(mb_hash, Schedule::default());
    db.set_mb_program_states(mb_hash, ProgramStates::default());
    db.mutate_mb_meta(mb_hash, |meta| {
        meta.computed = true;
        meta.last_advanced_eb = H256::zero();
    });
    mb_hash
}

/// Set up an MB chain with the supplied per-MB outcomes and update
/// `globals.latest_finalized_mb_hash` to the head. Returns the MB
/// hashes in chronological order.
fn setup_mb_chain(db: &Database, outcomes: Vec<Vec<StateTransition>>) -> Vec<H256> {
    let mut parent = H256::zero();
    let mut hashes = Vec::with_capacity(outcomes.len());
    for (i, outcome) in outcomes.into_iter().enumerate() {
        let h = append_mb(db, parent, (i + 1) as u64, outcome);
        hashes.push(h);
        parent = h;
    }
    db.globals_mutate(|g| g.latest_finalized_mb_hash = parent);
    hashes
}

fn nonempty_transition(seed: u8) -> StateTransition {
    StateTransition {
        actor_id: ActorId::from([seed; 32]),
        new_state_hash: H256::from([seed; 32]),
        exited: false,
        inheritor: ActorId::zero(),
        value_to_receive: seed as u128,
        value_to_receive_negative_sign: false,
        value_claims: vec![],
        messages: vec![],
    }
}

/// Build a batch from a small canonical setup so multiple tests can
/// share the scaffolding. Returns the chain head block plus the
/// resulting batch.
async fn prepare_canonical_batch(
    db: &Database,
) -> (SimpleBlockData, ethexe_common::gear::BatchCommitment) {
    let chain = test_block_chain(3).setup(db);
    let block = chain.blocks[3].to_simple();

    setup_mb_chain(
        db,
        vec![vec![nonempty_transition(1)], vec![nonempty_transition(2)]],
    );

    let manager = mock_batch_manager(db.clone());
    let batch = manager
        .create_batch_commitment(block)
        .await
        .expect("create_batch_commitment must not error")
        .expect("expected non-empty batch");
    (block, batch)
}

fn test_block_chain(len: u32) -> ethexe_common::mock::BlockChain {
    BlockChain::mock(len)
}

fn unwrap_rejected(status: ValidationStatus) -> ValidationRejectReason {
    match status {
        ValidationStatus::Rejected { reason, .. } => reason,
        ValidationStatus::Accepted(d) => panic!("expected rejection, got accepted with digest {d}"),
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[tokio::test]
async fn accepts_matching_request() {
    let db = Database::memory();
    let (block, batch) = prepare_canonical_batch(&db).await;

    let manager = mock_batch_manager(db);
    let expected_digest = batch.to_digest();
    let request = BatchCommitmentValidationRequest::new(&batch);
    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();

    match status {
        ValidationStatus::Accepted(digest) => assert_eq!(digest, expected_digest),
        ValidationStatus::Rejected { reason, .. } => {
            panic!("expected acceptance, got rejection: {reason:?}")
        }
    }
}

#[tokio::test]
async fn rejects_duplicate_code_ids() {
    let db = Database::memory();
    let (block, batch) = prepare_canonical_batch(&db).await;

    let manager = mock_batch_manager(db);

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    // Force duplicates: even an empty list with one repeated code is enough.
    let dup_id = CodeId::from([0xAA; 32]);
    request.codes = vec![dup_id, dup_id];

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::CodesHasDuplicates
    );
}

#[tokio::test]
async fn rejects_unknown_code_in_request() {
    let db = Database::memory();
    let (block, batch) = prepare_canonical_batch(&db).await;

    let manager = mock_batch_manager(db);
    let mut request = BatchCommitmentValidationRequest::new(&batch);

    let missing_code = CodeId::from(H256::random().to_fixed_bytes());
    request.codes.push(missing_code);

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::CodeNotWaitingForCommitment(missing_code)
    );
}

#[tokio::test]
async fn rejects_code_not_processed_yet() {
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();
    setup_mb_chain(&db, vec![vec![nonempty_transition(1)]]);

    // Queue a code id but don't mark it valid → "code not processed yet".
    let pending_code = CodeId::generate(b"pending");
    db.mutate_block_meta(block.hash, |meta| {
        meta.codes_queue
            .as_mut()
            .expect("codes_queue must exist after BlockChain::setup")
            .push_back(pending_code);
    });

    let manager = mock_batch_manager(db.clone());
    let batch = manager
        .clone()
        .create_batch_commitment(block)
        .await
        .unwrap()
        .expect("expected non-empty batch");

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    // create_batch_commitment skips codes without `code_valid`, so we
    // append it manually here to force aggregate_code_commitments to see it.
    request.codes.push(pending_code);

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::CodeIsNotProcessedYet(pending_code)
    );
}

#[tokio::test]
async fn rejects_digest_mismatch() {
    let db = Database::memory();
    let (block, batch) = prepare_canonical_batch(&db).await;

    let manager = mock_batch_manager(db);
    let mut request = BatchCommitmentValidationRequest::new(&batch);
    let original = request.digest;
    let mut wrong = original;
    while wrong == original {
        wrong = Digest::random();
    }
    request.digest = wrong;

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert!(matches!(
        unwrap_rejected(status),
        ValidationRejectReason::BatchDigestMismatch { expected, found }
        if expected == wrong && found == original
    ));
}

#[tokio::test]
async fn rejects_head_mb_not_finalized_locally() {
    let db = Database::memory();
    let (block, batch) = prepare_canonical_batch(&db).await;

    let manager = mock_batch_manager(db);

    let mut request = BatchCommitmentValidationRequest::new(&batch);
    // Substitute the head MB with one that has no `meta.finalized = true`
    // record locally — the manager must reject without signing.
    let foreign_head = H256::from([0xFE; 32]);
    request.head = Some(foreign_head);

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::HeadMbNotFinalized(foreign_head)
    );
}

#[tokio::test]
async fn rejects_head_mb_at_or_below_last_committed_mb() {
    // The coordinator must always advance past `last_committed_mb`. If
    // its `head_mb` lands at or below that height, the participant rejects
    // — re-committing a prefix would either no-op or fork on Router.
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();

    let mb_hashes = setup_mb_chain(
        &db,
        vec![vec![nonempty_transition(1)], vec![nonempty_transition(2)]],
    );
    let head = mb_hashes.last().copied().unwrap();

    let manager = mock_batch_manager(db.clone());
    let batch = manager
        .clone()
        .create_batch_commitment(block)
        .await
        .unwrap()
        .expect("expected non-empty batch");
    let request = BatchCommitmentValidationRequest::new(&batch);

    // Pretend we already committed up to `head` — height now matches
    // `last_committed_mb.height`, so the request can't advance.
    db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_mb = Some(head);
    });

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::HeadMbAlreadyCommitted(head)
    );
}

#[tokio::test]
async fn rejects_head_mb_not_computed() {
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();

    let mb_hashes = setup_mb_chain(
        &db,
        vec![vec![nonempty_transition(1)], vec![nonempty_transition(2)]],
    );

    let manager = mock_batch_manager(db.clone());
    // Build a batch first (head MB is computed).
    let batch = manager
        .clone()
        .create_batch_commitment(block)
        .await
        .unwrap()
        .expect("expected non-empty batch");
    let request = BatchCommitmentValidationRequest::new(&batch);

    // Now flip the head MB to "not computed" — the manager must
    // reject because it cannot trust the outcome.
    let head = mb_hashes.last().copied().unwrap();
    db.mutate_mb_meta(head, |meta| {
        meta.computed = false;
    });

    let status = manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::HeadMbNotComputed(head)
    );
}

#[tokio::test]
async fn rejects_empty_batch_request() {
    // No MBs and no committed codes → batch must be skipped on the
    // build side. Constructing a "request" out of an empty
    // BatchCommitment just to check that validation rejects it.
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();

    // No MBs in the chain at all (latest_finalized_mb_hash stays zero),
    // and no codes pending.
    let manager = mock_batch_manager(db.clone());
    let batch = manager
        .clone()
        .create_batch_commitment(block)
        .await
        .unwrap();
    assert!(batch.is_none(), "empty inputs must produce no batch");

    // Synthesize an "empty" request anyway and feed it to validate.
    let synthesized = BatchCommitmentValidationRequest {
        digest: Digest::random(),
        head: None,
        codes: Vec::new(),
        rewards: false,
        validators: false,
    };
    let status = manager
        .validate_batch_commitment(block, synthesized)
        .await
        .unwrap();
    assert_eq!(unwrap_rejected(status), ValidationRejectReason::EmptyBatch);
}

#[tokio::test]
async fn batch_size_limit_exceeded_is_rejected_on_validation() {
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();

    // Pile up a chain of MBs with many transitions each so the squashed
    // batch easily exceeds a tight size limit.
    let mut outcomes = Vec::new();
    for mb_idx in 0..5u8 {
        let mut o = Vec::new();
        for actor in 0..40u8 {
            // distinct actor per transition so squashing keeps them all
            o.push(nonempty_transition(mb_idx * 50 + actor + 1));
        }
        outcomes.push(o);
    }
    setup_mb_chain(&db, outcomes);

    // First build under a generous limit, then validate under a tight
    // one — that's how the manager catches an oversize batch from a
    // misbehaving coordinator.
    let big_manager = mock_batch_manager_with_limits(
        db.clone(),
        BatchLimits {
            commitment_delay_limit: std::num::NonZero::new(100).unwrap(),
            batch_size_limit: BLOCK_GAS_LIMIT, // large
            // Large enough that the checkpoint path doesn't fire in this size-limit scenario.
            uncommitted_chain_len_threshold: NonZero::new(u32::MAX).unwrap(),
        },
    );
    let batch = big_manager
        .create_batch_commitment(block)
        .await
        .unwrap()
        .expect("expected non-empty batch");
    let request = BatchCommitmentValidationRequest::new(&batch);

    let strict_manager = mock_batch_manager_with_limits(
        db,
        BatchLimits {
            commitment_delay_limit: std::num::NonZero::new(100).unwrap(),
            batch_size_limit: 256, // intentionally tiny
            // Large enough that the checkpoint path doesn't fire in this size-limit scenario.
            uncommitted_chain_len_threshold: NonZero::new(u32::MAX).unwrap(),
        },
    );
    let status = strict_manager
        .validate_batch_commitment(block, request)
        .await
        .unwrap();
    assert_eq!(
        unwrap_rejected(status),
        ValidationRejectReason::BatchSizeLimitExceeded
    );
}

#[tokio::test]
async fn squash_orders_negative_value_transitions_first() {
    // Two actors, two MBs each. Negative value (sender returning value
    // to the router) must come ahead of positive value (receiver) so
    // the on-chain pull-then-push order keeps the router solvent.
    let db = Database::memory();
    let chain = test_block_chain(3).setup(&db);
    let block = chain.blocks[3].to_simple();

    let actor_negative = ActorId::from([0xA1; 32]);
    let actor_positive = ActorId::from([0xB2; 32]);

    let transition = |actor_id: ActorId,
                      new_state_hash: H256,
                      value_to_receive: u128,
                      value_to_receive_negative_sign: bool| StateTransition {
        actor_id,
        new_state_hash,
        exited: false,
        inheritor: ActorId::zero(),
        value_to_receive,
        value_to_receive_negative_sign,
        value_claims: vec![],
        messages: vec![],
    };

    let mb1_neg = transition(actor_negative, H256::from([1; 32]), 70, true);
    let mb1_pos = transition(actor_positive, H256::from([2; 32]), 30, false);
    let mb2_neg = transition(actor_negative, H256::from([3; 32]), 20, false);
    let mb2_pos = transition(actor_positive, H256::from([4; 32]), 10, false);

    setup_mb_chain(&db, vec![vec![mb1_neg, mb1_pos], vec![mb2_neg, mb2_pos]]);

    let manager = mock_batch_manager(db.clone());
    let batch = manager
        .clone()
        .create_batch_commitment(block)
        .await
        .unwrap()
        .expect("expected non-empty batch");

    let chain_commitment = batch.chain_commitment.as_ref().expect("chain commitment");
    assert_eq!(
        chain_commitment
            .transitions
            .iter()
            .map(|t| t.actor_id)
            .collect::<Vec<_>>(),
        vec![actor_negative, actor_positive],
        "negative-sign actor must come first after sort"
    );
    assert_eq!(chain_commitment.transitions[0].value_to_receive, 50);
    assert!(chain_commitment.transitions[0].value_to_receive_negative_sign);
    assert_eq!(chain_commitment.transitions[1].value_to_receive, 40);
    assert!(!chain_commitment.transitions[1].value_to_receive_negative_sign);

    // And the round-trip must accept.
    let expected = batch.to_digest();
    let status = manager
        .validate_batch_commitment(block, BatchCommitmentValidationRequest::new(&batch))
        .await
        .unwrap();
    match status {
        ValidationStatus::Accepted(d) => assert_eq!(d, expected),
        ValidationStatus::Rejected { reason, .. } => panic!("rejected: {reason:?}"),
    }
}

/// Idle network: all MBs in the uncommitted range have empty
/// outcomes, no codes/validators/rewards are due, and the head MB's
/// `last_advanced_eb` is only a small number of Eth blocks past the
/// on-chain anchor. Below the configured threshold there's nothing
/// worth pinning to L1 — `create_batch_commitment` MUST return `None`.
/// Without this gate a bug would let the producer emit a vacuous
/// empty-transitions batch every round and pay gas for nothing.
#[tokio::test]
async fn idle_chain_below_threshold_yields_no_batch_commitment() {
    let db = Database::memory();
    let chain = test_block_chain(6).setup(&db);
    let block = chain.blocks[6].to_simple();

    let mb_hashes = setup_mb_chain(&db, vec![vec![], vec![]]);
    let head_mb = *mb_hashes.last().expect("non-empty");

    // Anchor advance lands 2 Eth heights past the last committed anchor.
    let advanced = chain.blocks[4].hash;
    let last_committed_eb = chain.blocks[2].hash;
    db.mutate_mb_meta(head_mb, |m| m.last_advanced_eb = advanced);
    db.mutate_block_meta(block.hash, |m| {
        m.last_committed_eb = Some(last_committed_eb)
    });

    // gap = height(blocks[4]) - height(blocks[2]) = 2; threshold is much larger.
    let manager = mock_batch_manager_with_limits(
        db,
        BatchLimits {
            commitment_delay_limit: std::num::NonZero::new(16).unwrap(),
            batch_size_limit: BLOCK_GAS_LIMIT,
            uncommitted_chain_len_threshold: NonZero::new(10).unwrap(),
        },
    );

    let result = manager
        .create_batch_commitment(block)
        .await
        .expect("create_batch_commitment must not error");
    assert!(
        result.is_none(),
        "below-threshold idle chain must produce no batch commitment, got {result:?}",
    );
}

/// Idle network as above, but the gap between the head MB's
/// `last_advanced_eb` and the on-chain anchor now strictly exceeds the
/// threshold. The coordinator MUST emit a checkpoint batch with an
/// empty-transitions `ChainCommitment` that pins the new Ethereum
/// anchor — otherwise long quiet stretches strand the anchor and
/// downstream `compute_mb` keeps re-walking the same EB events.
#[tokio::test]
async fn idle_chain_above_threshold_emits_checkpoint_batch_commitment() {
    let db = Database::memory();
    let chain = test_block_chain(6).setup(&db);
    let block = chain.blocks[6].to_simple();

    let mb_hashes = setup_mb_chain(&db, vec![vec![], vec![]]);
    let head_mb = *mb_hashes.last().expect("non-empty");

    // gap = height(blocks[5]) - height(blocks[1]) = 4
    let advanced = chain.blocks[5].hash;
    let last_committed_eb = chain.blocks[1].hash;
    db.mutate_mb_meta(head_mb, |m| m.last_advanced_eb = advanced);
    db.mutate_block_meta(block.hash, |m| {
        m.last_committed_eb = Some(last_committed_eb)
    });

    let threshold = NonZero::new(2).unwrap();
    let manager = mock_batch_manager_with_limits(
        db,
        BatchLimits {
            commitment_delay_limit: std::num::NonZero::new(16).unwrap(),
            batch_size_limit: BLOCK_GAS_LIMIT,
            uncommitted_chain_len_threshold: threshold,
        },
    );

    let batch = manager
        .create_batch_commitment(block)
        .await
        .expect("create_batch_commitment must not error")
        .expect("above-threshold idle chain must produce a checkpoint batch commitment");

    let chain_commitment = batch
        .chain_commitment
        .as_ref()
        .expect("checkpoint batch must carry a chain commitment");
    assert!(
        chain_commitment.transitions.is_empty(),
        "checkpoint chain commitment must carry no state transitions, got {} transitions",
        chain_commitment.transitions.len(),
    );
    assert_eq!(
        chain_commitment.last_advanced_eth_block, advanced,
        "checkpoint must pin the head MB's last_advanced_eb on-chain",
    );
    assert_eq!(
        chain_commitment.head, head_mb,
        "checkpoint must reference the latest finalized MB",
    );
    assert!(
        batch.code_commitments.is_empty()
            && batch.validators_commitment.is_none()
            && batch.rewards_commitment.is_none(),
        "checkpoint scenario should only carry the chain commitment",
    );
}

#[tokio::test]
async fn test_aggregate_validators_commitment() {
    // Shorten era/election so block index 5 lands exactly at election
    // start for era 1 and block 15 lands at election start for era 2.
    //
    // Slot 10s, era 100s (10 slots), election 50s (5 slots) ⇒
    //   era 0 covers ts ∈ [genesis, genesis+100); election for era 1
    //   opens at genesis+50.
    //   era 1 covers ts ∈ [genesis+100, genesis+200); election for
    //   era 2 opens at genesis+150.
    //
    // BlockChain::mock(20) emits blocks at ts = genesis_ts + i*slot for
    // i = chain index, so blocks[5] hits the era-1 election start and
    // blocks[15] hits the era-2 election start.
    let db = Database::memory();
    let mut chain = test_block_chain(20);
    {
        let mut config = chain.config.clone();
        config.timelines.era = NonZeroU64::new(10 * config.timelines.slot.get()).unwrap();
        config.timelines.election = 5 * config.timelines.slot.get();
        chain.config = config;
    }
    let chain = chain.setup(&db);
    // Force the config back into the in-memory DB (BlockChain::setup
    // wrote the original config first; we want the shortened one).
    db.set_config(chain.config.clone());

    let validators1: ValidatorsVec = vec![Address([1; 20]), Address([2; 20]), Address([3; 20])]
        .try_into()
        .unwrap();
    let validators2: ValidatorsVec = vec![Address([4; 20]), Address([5; 20]), Address([6; 20])]
        .try_into()
        .unwrap();

    let (manager, election) =
        mock_batch_manager_with_limits_and_election(db.clone(), BatchLimits::default());
    let timelines = chain.config.timelines;

    election
        .set_predefined_election_at(
            timelines.era_election_start_ts(0).unwrap(),
            validators1.clone(),
        )
        .await;
    election
        .set_predefined_election_at(
            timelines.era_election_start_ts(1).unwrap(),
            validators2.clone(),
        )
        .await;

    // Before election start (era 0, ts < genesis+50) → no commitment.
    let commitment = manager
        .aggregate_validators_commitment(&chain.blocks[4].to_simple())
        .await
        .unwrap();
    assert!(commitment.is_none(), "expected None before election period");

    // Right at election start for era 1 → commits validators1.
    let commitment = manager
        .aggregate_validators_commitment(&chain.blocks[5].to_simple())
        .await
        .unwrap()
        .expect("validators commitment expected");
    assert_eq!(commitment.validators, validators1);
    assert_eq!(commitment.era_index, 1);
    assert_eq!(commitment.aggregated_public_key.x, U256::zero());
    assert_eq!(commitment.aggregated_public_key.y, U256::zero());
    assert!(commitment.verifiable_secret_sharing_commitment.is_empty());

    // Inside era 1 election period → still validators1.
    let commitment = manager
        .aggregate_validators_commitment(&chain.blocks[7].to_simple())
        .await
        .unwrap()
        .expect("validators commitment expected");
    assert_eq!(commitment.validators, validators1);
    assert_eq!(commitment.era_index, 1);
    assert_eq!(commitment.aggregated_public_key.x, U256::zero());
    assert_eq!(commitment.aggregated_public_key.y, U256::zero());
    assert!(commitment.verifiable_secret_sharing_commitment.is_empty());

    // Mark era 1 already committed for `block 7` → manager skips.
    db.mutate_block_meta(chain.blocks[7].hash, |meta| {
        meta.latest_era_validators_committed = Some(1);
    });
    let commitment = manager
        .aggregate_validators_commitment(&chain.blocks[7].to_simple())
        .await
        .unwrap();
    assert!(
        commitment.is_none(),
        "expected None when next-era validators already committed"
    );

    // At era-2 election start with only era 0 marked committed: warns
    // about missed era 1 but still commits validators2 for era 2.
    db.mutate_block_meta(chain.blocks[15].hash, |meta| {
        meta.latest_era_validators_committed = Some(0);
    });
    let commitment = manager
        .aggregate_validators_commitment(&chain.blocks[15].to_simple())
        .await
        .unwrap()
        .expect("validators commitment expected");
    assert_eq!(commitment.validators, validators2);
    assert_eq!(commitment.era_index, 2);
    assert_eq!(commitment.aggregated_public_key.x, U256::zero());
    assert_eq!(commitment.aggregated_public_key.y, U256::zero());
    assert!(commitment.verifiable_secret_sharing_commitment.is_empty());

    // Bookkeeping past the next era is restricted — must error out.
    db.mutate_block_meta(chain.blocks[15].hash, |meta| {
        meta.latest_era_validators_committed = Some(3);
    });
    manager
        .aggregate_validators_commitment(&chain.blocks[15].to_simple())
        .await
        .unwrap_err();
}
