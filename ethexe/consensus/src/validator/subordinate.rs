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

use super::{
    DefaultProcessing, PendingEvent, StateHandler, ValidatorContext, ValidatorState,
    initial::Initial,
};
use crate::{
    ConsensusEvent,
    announces::{self, AnnounceRejectionReason, AnnounceStatus},
    validator::participant::Participant,
};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, Announce, HashOf, PromisePolicy, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::AnnounceStorageRO,
};
use std::mem;

/// In order to avoid too big size of pending events queue,
/// subordinate state handler removes redundant pending events
/// and also removes old events if we overflow this limit:
const MAX_PENDING_EVENTS: usize = 10;

/// [`Subordinate`] is the state of the validator which is not a producer.
/// It waits for the producer block, the waits for the block computing
/// and then switches to [`Participant`] state.
///
/// After computing the base announce, the subordinate loops back to
/// `WaitingForAnnounce` to accept mini-announces from the same producer.
/// When a validation request arrives whose head announce is already computed,
/// the subordinate transitions to `Participant`.
#[derive(Debug, Display)]
#[display("SUBORDINATE in {:?}", self.state)]
pub struct Subordinate {
    ctx: ValidatorContext,
    producer: Address,
    block: SimpleBlockData,
    is_validator: bool,
    state: State,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingForAnnounce,
    WaitingAnnounceComputed { announce_hash: HashOf<Announce> },
}

impl StateHandler for Subordinate {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut super::ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_computed_announce(
        mut self,
        computed_announce_hash: HashOf<Announce>,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingAnnounceComputed { announce_hash }
                if *announce_hash == computed_announce_hash =>
            {
                // Announce computed. Loop back to WaitingForAnnounce to accept
                // mini-announces or validation requests from the producer.
                self.state = State::WaitingForAnnounce;

                // Check pending for VR or mini-announce that arrived during computation.
                self.process_pending_after_compute()
            }
            _ => DefaultProcessing::computed_announce(self, computed_announce_hash),
        }
    }

    fn process_announce(mut self, verified_announce: VerifiedAnnounce) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingForAnnounce
                if verified_announce.address() == self.producer
                    && verified_announce.data().block_hash == self.block.hash =>
            {
                let (announce, _pub_key) = verified_announce.clone().into_parts();
                match announces::accept_announce(&self.ctx.core.db, announce.clone())? {
                    AnnounceStatus::Accepted(announce_hash) => {
                        self.ctx
                            .output(ConsensusEvent::AnnounceAccepted(announce_hash));
                        self.ctx.output(ConsensusEvent::ComputeAnnounce(
                            announce,
                            PromisePolicy::Disabled,
                        ));
                        self.state = State::WaitingAnnounceComputed { announce_hash };
                        Ok(self.into())
                    }
                    AnnounceStatus::Rejected {
                        reason: AnnounceRejectionReason::UnknownParent { .. },
                        ..
                    } => {
                        // Parent not yet included — defer to pending.
                        // Gossip reordering can cause the child to arrive before the parent.
                        if self.ctx.pending_events.len() < MAX_PENDING_EVENTS {
                            tracing::trace!(
                                "Announce parent not yet included, deferring to pending"
                            );
                            self.ctx.pending(verified_announce);
                        } else {
                            tracing::trace!(
                                "Announce parent not yet included but pending queue full, dropping"
                            );
                        }
                        Ok(self.into())
                    }
                    AnnounceStatus::Rejected { announce, reason } => {
                        self.ctx
                            .output(ConsensusEvent::AnnounceRejected(announce.to_hash()));
                        self.warning(format!(
                            "Received announce {announce:?} is rejected: {reason:?}"
                        ));
                        Initial::create(self.ctx)
                    }
                }
            }
            _ => DefaultProcessing::announce_from_producer(self, verified_announce),
        }
    }

    fn process_validation_request(
        mut self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingForAnnounce if request.address() == self.producer && self.is_validator => {
                // Check if VR's head announce is already computed.
                let head_computed = request
                    .data()
                    .head
                    .is_none_or(|h| self.ctx.core.db.announce_meta(h).computed);

                if head_computed {
                    // All announces computed, ready to validate.
                    self.ctx.pending(request);
                    Participant::create(self.ctx, self.block, self.producer)
                } else {
                    // VR arrived before its head announce was computed.
                    // Save to pending — will be retried after next announce computes.
                    tracing::trace!(
                        "VR head announce not yet computed, deferring to pending"
                    );
                    self.ctx.pending(request);
                    Ok(self.into())
                }
            }
            State::WaitingForAnnounce if request.address() == self.producer => {
                // Non-validator: VR is meaningless, drop it.
                Ok(self.into())
            }
            _ if request.address() == self.producer => {
                tracing::trace!(
                    "Receive validation request from producer: {request:?}, saved for later."
                );
                self.ctx.pending(request);
                Ok(self.into())
            }
            _ => DefaultProcessing::validation_request(self, request),
        }
    }
}

impl Subordinate {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
        is_validator: bool,
    ) -> Result<ValidatorState> {
        let mut earlier_announce = None;

        // Search for already received producer blocks.
        // If events amount is eq to MAX_PENDING_EVENTS, then oldest ones would be removed.
        // TODO #4641: potential abuse can be here. If we receive a lot of fake events,
        // important ones can be removed. What to do:
        // 1) Check event is sent by current or next or previous era validator.
        // 2) Malicious validator can send a lot of events (consider what to do).
        for event in mem::take(&mut ctx.pending_events) {
            match event {
                PendingEvent::Announce(validated_pb)
                    if earlier_announce.is_none()
                        && (validated_pb.data().block_hash == block.hash)
                        && validated_pb.address() == producer =>
                {
                    earlier_announce = Some(validated_pb.into_parts().0);
                }
                event if ctx.pending_events.len() < MAX_PENDING_EVENTS => {
                    // Events are sorted from newest to oldest,
                    // so we need to push back here in order to keep the order.
                    ctx.pending_events.push_back(event);
                }
                _ => {
                    tracing::trace!("Skipping pending event: {event:?}");
                }
            }
        }

        let state = Self {
            ctx,
            producer,
            block,
            is_validator,
            state: State::WaitingForAnnounce,
        };

        if let Some(announce) = earlier_announce {
            state.send_announce_for_computation(announce)
        } else {
            Ok(state.into())
        }
    }

    /// After an announce computes, check pending events for:
    /// - Mini-announces that can now be accepted (parent just got included)
    /// - Validation requests whose head is now computed
    fn process_pending_after_compute(mut self) -> Result<ValidatorState> {
        let pending = mem::take(&mut self.ctx.pending_events);
        let mut state: ValidatorState = self.into();

        // Process oldest-first so parent announces are handled before children.
        for event in pending.into_iter().rev() {
            state = match event {
                PendingEvent::Announce(announce) => state.process_announce(announce)?,
                PendingEvent::ValidationRequest(request) => {
                    state.process_validation_request(request)?
                }
            };
        }

        Ok(state)
    }

    fn send_announce_for_computation(mut self, announce: Announce) -> Result<ValidatorState> {
        match announces::accept_announce(&self.ctx.core.db, announce.clone())? {
            AnnounceStatus::Accepted(announce_hash) => {
                self.ctx
                    .output(ConsensusEvent::AnnounceAccepted(announce_hash));
                self.ctx.output(ConsensusEvent::ComputeAnnounce(
                    announce,
                    PromisePolicy::Disabled,
                ));
                self.state = State::WaitingAnnounceComputed { announce_hash };

                Ok(self.into())
            }
            AnnounceStatus::Rejected { announce, reason } => {
                self.ctx
                    .output(ConsensusEvent::AnnounceRejected(announce.to_hash()));
                self.warning(format!(
                    "Received announce {announce:?} is rejected: {reason:?}"
                ));

                Initial::create(self.ctx)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::{Announce, HashOf, consensus::BatchCommitmentValidationRequest, mock::*};
    use gprimitives::H256;
    use gsigner::PublicKey;

    fn verified_announce(
        signer: &gsigner::secp256k1::Signer,
        pub_key: PublicKey,
        block_hash: H256,
        parent: HashOf<Announce>,
    ) -> VerifiedAnnounce {
        signer.verified_test_data(pub_key, test_announce(block_hash, parent))
    }

    fn verified_request(
        signer: &gsigner::secp256k1::Signer,
        pub_key: PublicKey,
        block_hash: H256,
    ) -> VerifiedValidationRequest {
        signer.verified_test_data(
            pub_key,
            BatchCommitmentValidationRequest::new(&test_batch_commitment(block_hash, 1)),
        )
    }

    #[test]
    fn create_empty() {
        let (ctx, pub_keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = pub_keys[0];
        let block = test_simple_block_data(1);

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());
        assert_eq!(s.context().pending_events, vec![]);
    }

    #[test]
    fn earlier_received_announces() {
        let (mut ctx, keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = keys[0];
        let chain = test_block_chain(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();
        let parent_announce_hash = chain.block_top_announce_hash(0);
        let announce1 =
            verified_announce(&ctx.core.signer, producer, block.hash, parent_announce_hash);
        let announce2 =
            verified_announce(&ctx.core.signer, keys[1], block.hash, parent_announce_hash);

        ctx.pending(PendingEvent::Announce(announce1.clone()));
        ctx.pending(PendingEvent::Announce(announce2.clone()));

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(
            s.context().output,
            vec![
                ConsensusEvent::AnnounceAccepted(announce1.data().to_hash()),
                ConsensusEvent::ComputeAnnounce(announce1.data().clone(), PromisePolicy::Disabled)
            ]
        );
        // announce2 must stay in pending events, because it's not from current producer.
        assert_eq!(
            s.context().pending_events,
            vec![PendingEvent::Announce(announce2)]
        );
    }

    #[test]
    fn create_with_validation_requests() {
        let (mut ctx, keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = keys[0];
        let alice = keys[1];
        let block = test_simple_block_data(2);
        let request1 = verified_request(&ctx.core.signer, producer, block.hash);
        let request2 = verified_request(&ctx.core.signer, alice, block.hash);

        ctx.pending(PendingEvent::ValidationRequest(request1.clone()));
        ctx.pending(PendingEvent::ValidationRequest(request2.clone()));

        // Subordinate waits for announce after creation, and does not process validation requests.
        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);
        assert_eq!(
            s.context().pending_events,
            vec![request2.into(), request1.into()]
        );
    }

    #[test]
    fn create_with_many_pending_events() {
        let (mut ctx, keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = keys[0];
        let alice = keys[1];
        let chain = test_block_chain(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();
        let announce = verified_announce(
            &ctx.core.signer,
            producer,
            block.hash,
            chain.block_top_announce_hash(0),
        );

        ctx.pending(announce.clone());

        // Fill with fake blocks
        for i in 0..10 * MAX_PENDING_EVENTS {
            let announce = verified_announce(
                &ctx.core.signer,
                alice,
                test_block_hash(100 + i as u64),
                HashOf::zero(),
            );
            ctx.pending(PendingEvent::Announce(announce));
        }

        // Subordinate sends announce to computation and waits for it.
        // All pending events except first MAX_PENDING_EVENTS will be removed.
        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(
            s.context().output,
            vec![
                ConsensusEvent::AnnounceAccepted(announce.data().to_hash()),
                ConsensusEvent::ComputeAnnounce(announce.data().clone(), PromisePolicy::Disabled)
            ]
        );
        assert_eq!(s.context().pending_events.len(), MAX_PENDING_EVENTS);
    }

    #[test]
    fn simple() {
        let (ctx, pub_keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = pub_keys[0];
        let chain = test_block_chain(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();
        let announce = verified_announce(
            &ctx.core.signer,
            producer,
            block.hash,
            chain.block_top_announce_hash(0),
        );

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);

        // After receiving valid announce - subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(
            s.context().output,
            vec![
                ConsensusEvent::AnnounceAccepted(announce.data().to_hash()),
                ConsensusEvent::ComputeAnnounce(announce.data().clone(), PromisePolicy::Disabled)
            ]
        );

        // After announce is computed, subordinate stays in WaitingForAnnounce
        // (ready for mini-announces or VR). No immediate Participant transition.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(
            s.is_subordinate(),
            "should stay subordinate after compute, got {s:?}"
        );
    }

    #[test]
    fn simple_not_validator() {
        let (ctx, pub_keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = pub_keys[0];
        let chain = test_block_chain(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();
        let parent_announce_hash = chain.block_top_announce_hash(0);
        let announce =
            verified_announce(&ctx.core.signer, producer, block.hash, parent_announce_hash);

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block, producer.to_address(), false).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);

        // After receiving valid announce - subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(
            s.context().output,
            vec![
                ConsensusEvent::AnnounceAccepted(announce.data().to_hash()),
                ConsensusEvent::ComputeAnnounce(announce.data().clone(), PromisePolicy::Disabled)
            ]
        );

        // After announce is computed, non-validator subordinate stays in WaitingForAnnounce too.
        // It will transition to Initial on the next new_head.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
    }

    #[test]
    fn create_with_multiple_announces() {
        let (mut ctx, keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = keys[0];
        let alice = keys[1];
        let block = test_block_chain(1).setup(&ctx.core.db).blocks[1].to_simple();
        let parent_announce_hash = ctx.core.db.top_announce_hash(block.header.parent_hash);
        let producer_announce =
            verified_announce(&ctx.core.signer, producer, block.hash, parent_announce_hash);
        let alice_announce =
            verified_announce(&ctx.core.signer, alice, block.hash, parent_announce_hash);

        ctx.pending(PendingEvent::Announce(producer_announce.clone()));
        ctx.pending(PendingEvent::Announce(alice_announce.clone()));

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert_eq!(
            s.context().output,
            vec![
                ConsensusEvent::AnnounceAccepted(producer_announce.data().to_hash()),
                ConsensusEvent::ComputeAnnounce(
                    producer_announce.data().clone(),
                    PromisePolicy::Disabled
                )
            ]
        );
        assert_eq!(s.context().pending_events, vec![alice_announce.into()]);
    }

    #[test]
    fn process_external_event_with_invalid_announce() {
        let (ctx, keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = keys[0];
        let alice = keys[1];
        let block = test_simple_block_data(3);
        let invalid_announce =
            verified_announce(&ctx.core.signer, alice, block.hash, HashOf::zero());

        let s = Subordinate::create(ctx, block, producer.to_address(), true)
            .unwrap()
            .process_announce(invalid_announce.clone())
            .unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(s.context().output[0], ConsensusEvent::Warning(_)));
        assert_eq!(s.context().pending_events, vec![invalid_announce.into()]);
    }

    #[test]
    fn process_computed_block_with_unexpected_hash() {
        let (ctx, pub_keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = pub_keys[0];
        let block = test_simple_block_data(4);

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        let s = s.process_computed_announce(HashOf::random()).unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(s.context().output[0], ConsensusEvent::Warning(_)));
    }

    #[test]
    fn defer_announce_with_unknown_parent() {
        let (ctx, pub_keys, _) = mock_validator_context(ethexe_db::Database::memory());
        let producer = pub_keys[0];
        let chain = test_block_chain(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        // Create an announce whose parent is NOT in DB (simulates gossip reordering).
        let announce_with_unknown_parent = Announce {
            block_hash: block.hash,
            parent: HashOf::random(),
            gas_allowance: Some(42),
            injected_transactions: vec![],
        };
        let announce = ctx
            .core
            .signer
            .verified_test_data(producer, announce_with_unknown_parent);

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");

        // Announce with unknown parent is deferred to pending (not rejected).
        let s = s.process_announce(announce).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output.len(), 0);
        assert_eq!(
            s.context().pending_events.len(),
            1,
            "Announce should be saved to pending for later replay"
        );
    }
}
