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
    ConsensusEvent, SignedAnnounce, SignedValidationRequest,
    validator::{core::ValidatorCore, participant::Participant},
};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{Address, Announce, AnnounceHash, SimpleBlockData};
use std::mem;

/// In order to avoid too big size of pending events queue,
/// subordinate state handler removes redundant pending events
/// and also removes old events if we overflow this limit:
const MAX_PENDING_EVENTS: usize = 10;

/// [`Subordinate`] is the state of the validator which is not a producer.
/// It waits for the producer block, the waits for the block computing
/// and then switches to [`Participant`] state.
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
    WaitingAnnounceComputed { announce_hash: AnnounceHash },
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
        self,
        computed_announce_hash: AnnounceHash,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingAnnounceComputed { announce_hash }
                if *announce_hash == computed_announce_hash =>
            {
                if self.is_validator {
                    Participant::create(self.ctx, self.block, self.producer)
                } else {
                    Initial::create(self.ctx)
                }
            }
            _ => DefaultProcessing::computed_announce(self, computed_announce_hash),
        }
    }

    fn process_announce(mut self, signed_announce: SignedAnnounce) -> Result<ValidatorState> {
        match &mut self.state {
            State::WaitingForAnnounce
                if signed_announce.address() == self.producer
                    && signed_announce.data().block_hash == self.block.hash =>
            {
                let (announce, _) = signed_announce.into_parts();
                self.send_announce_for_computation(announce)
            }
            _ => DefaultProcessing::block_from_producer(self, signed_announce),
        }
    }

    fn process_validation_request(
        mut self,
        request: SignedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.producer {
            log::trace!("Receive validation request from producer: {request:?}, saved for later.");
            self.ctx.pending(request);

            Ok(self.into())
        } else {
            DefaultProcessing::validation_request(self, request)
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
                PendingEvent::Announce(signed_pb)
                    if earlier_announce.is_none()
                        && (signed_pb.data().block_hash == block.hash)
                        && signed_pb.address() == producer =>
                {
                    earlier_announce = Some(signed_pb.into_parts().0);
                }
                event if ctx.pending_events.len() < MAX_PENDING_EVENTS => {
                    // Events are sorted from newest to oldest,
                    // so we need to push back here in order to keep the order.
                    ctx.pending_events.push_back(event);
                }
                _ => {
                    log::trace!("Skipping pending event: {event:?}");
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

    fn send_announce_for_computation(mut self, announce: Announce) -> Result<ValidatorState> {
        match self.ctx.core.accept_announce(announce.clone())? {
            AnnounceStatus::Accepted(announce_hash) => {
                self.ctx.output(ConsensusEvent::ComputeAnnounce(announce));
                self.state = State::WaitingAnnounceComputed { announce_hash };

                Ok(self.into())
            }
            AnnounceStatus::Rejected { announce, reason } => {
                log::warn!("Received announce {announce:?} is rejected: {reason}");
                Initial::create(self.ctx)
            }
        }
    }
}

enum AnnounceStatus {
    Accepted(AnnounceHash),
    Rejected { announce: Announce, reason: String },
}

impl ValidatorCore {
    fn accept_announce(&self, announce: Announce) -> Result<AnnounceStatus> {
        let best_parent = self.best_parent_announce(announce.block_hash)?;
        if best_parent != announce.parent {
            return Ok(AnnounceStatus::Rejected {
                announce,
                reason: format!("best parent is {best_parent}"),
            });
        }

        match self.include_announce(announce.clone()) {
            Ok(announce_hash) => Ok(AnnounceStatus::Accepted(announce_hash)),
            Err(err) => Ok(AnnounceStatus::Rejected {
                announce,
                reason: format!("{err}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SignedAnnounce, mock::*, validator::mock::*};
    use ethexe_common::mock::*;

    #[test]
    fn create_empty() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(());

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());
        assert_eq!(s.context().pending_events, vec![]);
    }

    #[test]
    fn earlier_received_announces() {
        let (mut ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let blocks = BlockChain::mock(1).setup(&ctx.core.db).blocks;
        let block = blocks[1].to_simple();
        let parent_announce_hash = blocks[0].as_prepared().announces.first().copied().unwrap();
        let announce1 = ctx
            .core
            .signer
            .mock_signed_data(producer, (block.hash, parent_announce_hash));
        let announce2 = ctx
            .core
            .signer
            .mock_signed_data(keys[1], (block.hash, parent_announce_hash));

        ctx.pending(PendingEvent::Announce(announce1.clone()));
        ctx.pending(PendingEvent::Announce(announce2.clone()));

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeAnnounce(announce1.data().clone())]
        );
        // announce2 must stay in pending events, because it's not from current producer.
        assert_eq!(
            s.context().pending_events,
            vec![PendingEvent::Announce(announce2)]
        );
    }

    #[test]
    fn create_with_validation_requests() {
        let (mut ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = SimpleBlockData::mock(());
        let request1 = ctx.core.signer.mock_signed_data(producer, ());
        let request2 = ctx.core.signer.mock_signed_data(alice, ());

        ctx.pending(PendingEvent::ValidationRequest(request1.clone()));
        ctx.pending(PendingEvent::ValidationRequest(request2.clone()));

        // Subordinate waits for announce after creation, and does not process validation requests.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);
        assert_eq!(
            s.context().pending_events,
            vec![request2.into(), request1.into()]
        );
    }

    #[test]
    fn create_with_many_pending_events() {
        let (mut ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let blocks = BlockChain::mock(1).setup(&ctx.core.db).blocks;
        let block = blocks[1].to_simple();
        let announce: SignedAnnounce = ctx.core.signer.mock_signed_data(
            producer,
            (
                block.hash,
                blocks[0].as_prepared().announces.first().copied().unwrap(),
            ),
        );

        ctx.pending(announce.clone());

        // Fill with fake blocks
        for _ in 0..10 * MAX_PENDING_EVENTS {
            let announce = ctx.core.signer.mock_signed_data(alice, block.hash);
            ctx.pending(PendingEvent::Announce(announce));
        }

        // Subordinate sends announce to computation and waits for it.
        // All pending events except first MAX_PENDING_EVENTS will be removed.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);
        assert_eq!(s.context().pending_events.len(), MAX_PENDING_EVENTS);
    }

    #[test]
    fn simple() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let blocks = BlockChain::mock(1).setup(&ctx.core.db).blocks;
        let parent_announce_hash = blocks[0].as_prepared().announces.first().copied().unwrap();
        let block = blocks[1].to_simple();
        let announce = ctx
            .core
            .signer
            .mock_signed_data(producer, (block.hash, parent_announce_hash));

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);

        // After receiving valid announce - subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);

        // After announce is computed, subordinate switches to participant state.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(s.is_participant(), "got {s:?}");
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);
    }

    #[test]
    fn simple_not_validator() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let blocks = BlockChain::mock(1).setup(&ctx.core.db).blocks;
        let block = blocks[1].to_simple();
        let parent_announce_hash = blocks[0].as_prepared().announces.first().copied().unwrap();
        let announce = ctx
            .core
            .signer
            .mock_signed_data(producer, (block.hash, parent_announce_hash));

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), false).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![]);

        // After receiving valid announce - subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate(), "got {s:?}");
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);

        // After announce is computed, not-validator subordinate switches to initial state.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(s.is_initial(), "got {s:?}");
    }

    #[test]
    fn create_with_multiple_announces() {
        let (mut ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let parent_announce_hash = ctx.core.db.top_announce_hash(block.header.parent_hash);
        let announce_producer = ctx
            .core
            .signer
            .mock_signed_data(producer, (block.hash, parent_announce_hash));
        let announce_alice = ctx
            .core
            .signer
            .mock_signed_data(alice, (block.hash, parent_announce_hash));

        ctx.pending(PendingEvent::Announce(announce_producer.clone()));
        ctx.pending(PendingEvent::Announce(announce_alice.clone()));

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert_eq!(
            s.context().output,
            vec![announce_producer.data().clone().into()]
        );
        assert_eq!(s.context().pending_events, vec![announce_alice.into()]);
    }

    #[test]
    fn process_external_event_with_invalid_announce() {
        let (ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = SimpleBlockData::mock(());
        let invalid_announce = ctx.core.signer.mock_signed_data(alice, block.hash);

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true)
            .unwrap()
            .process_announce(invalid_announce.clone())
            .unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(s.context().output[0], ConsensusEvent::Warning(_)));
        assert_eq!(s.context().pending_events, vec![invalid_announce.into()]);
    }

    #[test]
    fn process_computed_block_with_unexpected_hash() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(());

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        let s = s.process_computed_announce(AnnounceHash::random()).unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(s.context().output[0], ConsensusEvent::Warning(_)));
    }
}
