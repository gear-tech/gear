// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`Watcher`] is the non-signing twin of [`Participant`](super::Participant).
//!
//! Entered when the node has no validator key, or its key is not in the
//! current era's validator set. The watcher subscribes to the coordinator's
//! validation request just like a participant, re-derives the same batch via
//! `validate_batch_commitment` (which transitively populates the local
//! `outgoing_actions` cache — see
//! [`BatchCommitmentManager::persist_outgoing_actions`](super::batch::BatchCommitmentManager)),
//! and then returns to [`Idle`] without signing.
//!
//! The persistence side-effect is the whole point: RPC clients querying
//! `mirror.outgoing_actions(state_hash)` for merkle-proof building need the
//! `state_hash → value_claims` mapping in the local DB regardless of whether
//! the node was elected this round.

use super::{
    DefaultProcessing, PendingEvent, StateHandler, ValidatorContext, ValidatorState, idle::Idle,
};
use crate::validator::batch::ValidationStatus;
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, SimpleBlockData,
    consensus::{BatchCommitmentValidationRequest, VerifiedValidationRequest},
};
use futures::{FutureExt, future::BoxFuture};
use std::task::Poll;

#[derive(Debug, Display)]
#[display("WATCHER in state {state:?}")]
pub struct Watcher {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    coordinator: Address,
    state: State,
}

#[derive(Debug)]
enum State {
    WaitingForValidationRequest,
    ProcessingValidationRequest {
        #[debug(skip)]
        future: BoxFuture<'static, Result<ValidationStatus>>,
    },
}

impl StateHandler for Watcher {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.coordinator {
            self.process_coordinator_request(request.into_parts().0)
        } else {
            DefaultProcessing::validation_request(self, request)
        }
    }

    fn poll_next_state(
        mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Result<(Poll<()>, ValidatorState)> {
        if let State::ProcessingValidationRequest { future } = &mut self.state
            && let Poll::Ready(res) = future.poll_unpin(cx)
        {
            match res {
                Ok(ValidationStatus::Accepted(digest)) => {
                    // Re-derivation matched the coordinator's digest; the
                    // outgoing-actions cache was populated as a side effect of
                    // `validate_batch_commitment`. Nothing else to do here.
                    tracing::debug!(
                        block = %self.block.hash,
                        ?digest,
                        "watcher: batch accepted, outgoing actions persisted",
                    );
                }
                Ok(ValidationStatus::Rejected { request, reason }) => {
                    // Mismatch with the coordinator. Surface as a warning event
                    // so the operator notices a divergent local view, but do
                    // not propagate further — the watcher has no role to play.
                    self.warning(format!(
                        "rejected coordinator's batch {request:?}: {reason}"
                    ));
                }
                Err(err) => return Err(err),
            }

            Idle::create(self.ctx).map(|s| (Poll::Ready(()), s))
        } else {
            Ok((Poll::Pending, self.into()))
        }
    }
}

impl Watcher {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        coordinator: Address,
    ) -> Result<ValidatorState> {
        // Mirror Participant: drain at most one validation request from the
        // pending stash that already matches our coordinator.
        let mut earlier_validation_request = None;
        ctx.pending_events.retain(|event| match event {
            PendingEvent::ValidationRequest(signed_data)
                if earlier_validation_request.is_none() && signed_data.address() == coordinator =>
            {
                earlier_validation_request = Some(signed_data.data().clone());

                false
            }
            _ => true,
        });

        let watcher = Self {
            ctx,
            block,
            coordinator,
            state: State::WaitingForValidationRequest,
        };

        let Some(validation_request) = earlier_validation_request else {
            return Ok(watcher.into());
        };

        watcher.process_coordinator_request(validation_request)
    }

    fn process_coordinator_request(
        mut self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidatorState> {
        let State::WaitingForValidationRequest = self.state else {
            self.warning("unexpected validation request".to_string());
            return Ok(self.into());
        };

        self.state = State::ProcessingValidationRequest {
            future: self
                .ctx
                .core
                .batch_manager
                .clone()
                .validate_batch_commitment(self.block, request)
                .boxed(),
        };

        Ok(self.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::{
        ValidatorState,
        batch::BatchCommitmentManager,
        test_support::{count_publish_messages, drain_warnings, test_context},
    };
    use ethexe_common::{
        OutgoingAction, ProgramStates, Schedule,
        consensus::BatchCommitmentValidationRequest,
        db::{CompactMb, GlobalsStorageRW, MbStorageRW},
        ecdsa::{PrivateKey, SignedData},
        gear::{StateTransition, ValueClaim},
        malachite::{ProcessQueuesLimits, Transaction, Transactions},
        mock::{BlockChain, Mock},
    };
    use ethexe_db::Database;
    use ethexe_ethereum::middleware::{ElectionProvider, MockElectionProvider};
    use gprimitives::{ActorId, H256, MessageId};
    use std::task::{Context as PollContext, Waker};

    /// Mirror the helpers from `batch/tests.rs` so this test module is
    /// self-contained — keeps the cross-module surface small.
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

    fn transition_with_value_claims(seed: u8) -> StateTransition {
        StateTransition {
            actor_id: ActorId::from([seed; 32]),
            new_state_hash: H256::from([seed; 32]),
            exited: false,
            inheritor: ActorId::zero(),
            value_to_receive: seed as u128,
            value_to_receive_negative_sign: false,
            value_claims: vec![ValueClaim {
                message_id: MessageId::from([seed; 32]),
                destination: ActorId::from([0xCC; 32]),
                value: seed as u128,
            }],
            messages: vec![],
        }
    }

    /// Sign a validation request as if it came from `coordinator_pk`.
    fn signed_request(
        coordinator_pk: &PrivateKey,
        request: BatchCommitmentValidationRequest,
    ) -> VerifiedValidationRequest {
        SignedData::create(coordinator_pk, request)
            .expect("signing must succeed")
            .into_verified()
    }

    /// Produce a canonical batch via a producer-side run on `producer_db`.
    /// The same chain has to be replayed onto the verifier DB so the
    /// watcher's `validate_batch_commitment` re-derive lines up.
    async fn build_canonical_batch(producer_db: &Database) -> ethexe_common::gear::BatchCommitment {
        let chain = BlockChain::mock(3).setup(producer_db);
        let block = chain.blocks[3].to_simple();

        setup_mb_chain(producer_db, vec![vec![transition_with_value_claims(7)]]);

        let middleware = crate::validator::core::MiddlewareWrapper::from_inner(Box::new(
            MockElectionProvider::new(),
        )
            as Box<dyn ElectionProvider>);
        BatchCommitmentManager::new(
            crate::validator::batch::BatchLimits::default(),
            producer_db.clone(),
            middleware,
        )
        .create_batch_commitment(block)
        .await
        .expect("create must succeed")
        .expect("non-empty batch")
    }

    /// Set up a verifier DB with the same chain state (so `validate_batch_commitment`
    /// re-derives the same batch) but with no `outgoing_actions` pre-populated.
    fn setup_verifier_db() -> (Database, SimpleBlockData) {
        let db = Database::memory();
        let chain = BlockChain::mock(3).setup(&db);
        let block = chain.blocks[3].to_simple();
        setup_mb_chain(&db, vec![vec![transition_with_value_claims(7)]]);
        (db, block)
    }

    /// Pump `poll_next_state` on a state until it's pending or transitions
    /// away from Watcher. Returns the final state.
    fn pump_to_completion(mut state: ValidatorState) -> ValidatorState {
        let waker = Waker::noop();
        let mut cx = PollContext::from_waker(waker);
        // FuturesUnordered as a host for the poll, to let the boxed future
        // make progress. We have to manually advance the future via the
        // state's `poll_next_state` since that's the contract.
        for _ in 0..1024 {
            let (poll, next) = match state {
                ValidatorState::Watcher(w) => w.poll_next_state(&mut cx).unwrap(),
                other => return other,
            };
            state = next;
            if poll.is_pending() {
                // Spin briefly to let the boxed validation future make
                // progress. The future is purely synchronous re-derivation,
                // so a small number of polls suffices.
                std::thread::yield_now();
                continue;
            }
            return state;
        }
        panic!("pump_to_completion: state did not settle within budget");
    }

    #[tokio::test]
    async fn accept_path_persists_outgoing_actions_without_signing() {
        // Build the canonical batch on a separate DB so we have a valid
        // request payload to sign.
        let producer_db = Database::memory();
        let batch = build_canonical_batch(&producer_db).await;

        // Collect the (state_hash, claims) mappings we expect the watcher
        // to persist when it accepts the batch. Empty value_claims are
        // skipped — producer/persist helper skips them too.
        let expected_mappings: Vec<(H256, Vec<ValueClaim>)> = batch
            .chain_commitment
            .as_ref()
            .expect("chain commitment present")
            .transitions
            .iter()
            .filter(|t| !t.value_claims.is_empty())
            .map(|t| (t.new_state_hash, t.value_claims.clone()))
            .collect();
        assert!(!expected_mappings.is_empty(), "fixture must have claims");

        // Stand up a verifier DB and watcher; sign the request with a
        // fresh coordinator key.
        let (verifier_db, block) = setup_verifier_db();
        let coordinator_pk = PrivateKey::random();
        let coordinator_addr = coordinator_pk.public_key().to_address();

        let ctx = test_context(verifier_db.clone(), None);
        let initial = Watcher::create(ctx, block, coordinator_addr).unwrap();

        // Forward the signed request; watcher should enter the
        // ProcessingValidationRequest sub-state.
        let request = BatchCommitmentValidationRequest::new(&batch);
        let verified = signed_request(&coordinator_pk, request);
        let processing = initial.process_validation_request(verified).unwrap();
        assert!(matches!(&processing, ValidatorState::Watcher(_)));

        // Pump the state machine; the validation future completes
        // synchronously on a memory DB.
        let settled = pump_to_completion(processing);
        assert!(
            matches!(&settled, ValidatorState::Idle(_)),
            "watcher must return to Idle, got {settled}",
        );

        // The outgoing-actions side effect is the whole point of running
        // the watcher.
        use ethexe_common::db::OutgoingActionStorageRO;
        for (state_hash, value_claims) in &expected_mappings {
            let stored = verifier_db
                .outgoing_actions(*state_hash)
                .expect("watcher must persist outgoing_actions")
                .into_inner();
            let expected: Vec<OutgoingAction> = value_claims
                .iter()
                .cloned()
                .map(OutgoingAction::ValueClaim)
                .collect();
            assert_eq!(stored, expected);
        }

        // And watcher must never sign. Inspect the post-settlement context.
        let final_ctx = settled.into_context();
        assert_eq!(
            count_publish_messages(&final_ctx),
            0,
            "watcher must never emit PublishMessage",
        );
    }

    #[tokio::test]
    async fn reject_path_emits_warning_without_signing() {
        // Build a real batch, then corrupt the request so re-derive
        // produces a different digest → ValidationStatus::Rejected.
        let producer_db = Database::memory();
        let batch = build_canonical_batch(&producer_db).await;

        let (verifier_db, block) = setup_verifier_db();
        let coordinator_pk = PrivateKey::random();
        let coordinator_addr = coordinator_pk.public_key().to_address();

        let ctx = test_context(verifier_db.clone(), None);
        let initial = Watcher::create(ctx, block, coordinator_addr).unwrap();

        // Force a digest mismatch by appending a bogus code id; the
        // watcher's re-derive will reject with CodeNotWaitingForCommitment.
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        request.codes.push(gprimitives::CodeId::from([0xFA; 32]));
        let verified = signed_request(&coordinator_pk, request);
        let processing = initial.process_validation_request(verified).unwrap();

        let settled = pump_to_completion(processing);
        assert!(
            matches!(&settled, ValidatorState::Idle(_)),
            "watcher must return to Idle after rejecting, got {settled}",
        );

        let mut final_ctx = settled.into_context();
        let warnings = drain_warnings(&mut final_ctx);
        assert!(
            warnings.iter().any(|w| w.contains("rejected coordinator")),
            "rejected path must emit a watcher-tagged warning, got {warnings:?}",
        );
        assert_eq!(
            count_publish_messages(&final_ctx),
            0,
            "watcher must never emit PublishMessage even on rejection",
        );
    }

    #[tokio::test]
    async fn non_coordinator_request_is_stashed_as_pending() {
        let (verifier_db, block) = setup_verifier_db();
        let coordinator_pk = PrivateKey::random();
        let coordinator_addr = coordinator_pk.public_key().to_address();

        let ctx = test_context(verifier_db, None);
        let watcher = Watcher::create(ctx, block, coordinator_addr).unwrap();

        // Sign with a key *different* from the coordinator; the watcher
        // must NOT treat this as the coordinator's request — it falls
        // through to DefaultProcessing, which stashes it in `pending_events`
        // for later. The default handler also emits a warning.
        let other_pk = PrivateKey::random();
        let dummy_request = BatchCommitmentValidationRequest {
            digest: ethexe_common::Digest::zero(),
            head: None,
            codes: vec![],
            validators: false,
            rewards: false,
        };
        let verified = signed_request(&other_pk, dummy_request);
        let next = watcher.process_validation_request(verified).unwrap();

        // State must remain Watcher, not transition to Processing.
        let ValidatorState::Watcher(w) = &next else {
            panic!("expected Watcher state, got {next}");
        };
        assert!(
            matches!(w.state, State::WaitingForValidationRequest),
            "no-match must keep us in WaitingForValidationRequest, got {:?}",
            w.state,
        );
        assert_eq!(
            w.ctx.pending_events.len(),
            1,
            "non-coordinator request must be stashed for later",
        );
    }
}
