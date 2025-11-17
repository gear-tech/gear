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
    ConsensusEvent, utils,
    validator::{
        participant::Participant,
        tx_pool::{TxValidity, TxValidityChecker},
    },
};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::{AnnounceStorageRW, BlockMetaStorageRW, InjectedStorageRW},
};
use gprimitives::H256;
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

#[derive(Clone, PartialEq, Eq)]
enum AnnounceValidity {
    // Announce is valid and can be send to computation.
    Valid,
    // Announce is not valid and will be rejected.
    Invalid(String),
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingForAnnounceAndBlockPrepared {
        block_prepared: bool,
        received_announce: Option<Announce>,
    },
    WaitingAnnounceComputed {
        announce_hash: HashOf<Announce>,
    },
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

    fn process_prepared_block(mut self, block_hash: H256) -> Result<ValidatorState> {
        if block_hash != self.block.hash {
            return DefaultProcessing::prepared_block(self, block_hash);
        }

        match &mut self.state {
            State::WaitingForAnnounceAndBlockPrepared {
                block_prepared,
                received_announce,
            } => {
                if *block_prepared {
                    tracing::warn!("Receive block {block_hash} prepared twice or more, ignoring");
                    return Ok(self.into());
                }

                utils::propagate_announces_for_skipped_blocks(
                    &self.ctx.core.db,
                    self.block.header.parent_hash,
                )?;

                *block_prepared = true;

                if let Some(announce) = received_announce.take() {
                    self.send_announce_for_computation(announce)
                } else {
                    Ok(self.into())
                }
            }
            _ => DefaultProcessing::prepared_block(self, block_hash),
        }
    }

    fn process_computed_announce(
        self,
        computed_announce_hash: HashOf<Announce>,
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

    fn process_announce(mut self, validated_announce: VerifiedAnnounce) -> Result<ValidatorState> {
        match &mut self.state {
            State::WaitingForAnnounceAndBlockPrepared {
                block_prepared,
                received_announce,
                ..
            } if received_announce.is_none()
                && validated_announce.address() == self.producer
                && validated_announce.data().block_hash == self.block.hash =>
            {
                let (announce, _pub_key) = validated_announce.into_parts();

                if *block_prepared {
                    self.send_announce_for_computation(announce)
                } else {
                    *received_announce = Some(announce);
                    Ok(self.into())
                }
            }
            _ => DefaultProcessing::block_from_producer(self, validated_announce),
        }
    }

    fn process_validation_request(
        mut self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.producer {
            tracing::trace!(
                "Receive validation request from producer: {request:?}, saved for later."
            );
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

        let state = State::WaitingForAnnounceAndBlockPrepared {
            block_prepared: false,
            received_announce: earlier_announce,
        };

        Ok(Self {
            ctx,
            producer,
            block,
            is_validator,
            state,
        }
        .into())
    }

    fn send_announce_for_computation(mut self, announce: Announce) -> Result<ValidatorState> {
        let parent =
            utils::parent_main_line_announce(&self.ctx.core.db, self.block.header.parent_hash)?;

        if parent != announce.parent {
            self.warning(format!(
                "Received announce {announce:?} is from invalid branch, expected parent is {parent}",
            ));
            return Initial::create(self.ctx);
        }

        let announce_hash = self.ctx.core.db.set_announce(announce.clone());
        self.ctx
            .core
            .db
            .mutate_block_meta(announce.block_hash, |meta| {
                meta.announces.get_or_insert_default().insert(announce_hash);
            });

        match self.verify_announce(&announce)? {
            AnnounceValidity::Valid => {
                self.ctx.output(ConsensusEvent::ComputeAnnounce(announce));
                self.state = State::WaitingAnnounceComputed { announce_hash };
                Ok(self.into())
            }
            AnnounceValidity::Invalid(reason) => {
                self.ctx.warning(reason);
                Initial::create(self.ctx)
            }
        }
    }

    fn verify_announce(&self, announce: &Announce) -> Result<AnnounceValidity> {
        // Verify for parent announce, because of the current is not processed.
        let tx_checker = TxValidityChecker::new_for_announce(
            self.ctx.core.db.clone(),
            self.block.hash,
            announce.parent,
        )?;

        for tx in announce.injected_transactions.iter() {
            let validity_status = tx_checker.check_tx_validity(tx)?;

            match validity_status {
                TxValidity::Valid => {
                    self.ctx.core.db.set_injected_transaction(tx.clone());
                }

                _ => {
                    tracing::trace!(
                        announce = ?announce.to_hash(),
                        "announce contains invalid transtion with status {validity_status:?}, rejecting announce."
                    );

                    return Ok(AnnounceValidity::Invalid(format!(
                        "announce({:?}) contains an invalid injected tx, reject it.",
                        announce.to_hash()
                    )));
                }
            }
        }

        Ok(AnnounceValidity::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::mock::*;

    #[test]
    fn create_empty() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(());

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
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
            .mock_verified_data(producer, (block.hash, parent_announce_hash));
        let announce2 = ctx
            .core
            .signer
            .mock_verified_data(keys[1], (block.hash, parent_announce_hash));

        ctx.pending(PendingEvent::Announce(announce1.clone()));
        ctx.pending(PendingEvent::Announce(announce2.clone()));

        // Subordinate waits for block prepared after creation.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());

        // After receiving block prepared, subordinate create a task to compute earlier received announce1.
        let s = s.process_prepared_block(block.hash).unwrap();
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeAnnounce(announce1.data().clone())]
        );
        assert!(s.is_subordinate());

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
        let request1 = ctx.core.signer.mock_verified_data(producer, ());
        let request2 = ctx.core.signer.mock_verified_data(alice, ());

        ctx.pending(PendingEvent::ValidationRequest(request1.clone()));
        ctx.pending(PendingEvent::ValidationRequest(request2.clone()));

        // Subordinate waits for block prepared and announce after creation, and does not process validation requests.
        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());
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
        let announce: VerifiedAnnounce = ctx.core.signer.mock_verified_data(
            producer,
            (
                block.hash,
                blocks[0].as_prepared().announces.first().copied().unwrap(),
            ),
        );

        ctx.pending(announce.clone());

        // Fill with fake blocks
        for _ in 0..10 * MAX_PENDING_EVENTS {
            let announce = ctx.core.signer.mock_verified_data(alice, block.hash);
            ctx.pending(PendingEvent::Announce(announce));
        }

        // After block prepared, subordinate sends announce to computation and waits for it.
        // All pending events except first MAX_PENDING_EVENTS will be removed.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();
        assert!(s.is_subordinate());
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
            .mock_verified_data(producer, (block.hash, parent_announce_hash));

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());

        // Block is prepared, but announce is not received yet.
        let s = s.process_prepared_block(block.hash).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());

        // Announce is received, so subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate());
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);

        // After announce is computed, subordinate switches to participant state.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(s.is_participant());
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
            .mock_verified_data(producer, (block.hash, parent_announce_hash));

        // Subordinate waits for block prepared and announce after creation.
        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), false).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());

        // Block is prepared, but announce is not received yet.
        let s = s.process_prepared_block(block.hash).unwrap();
        assert!(s.is_subordinate());
        assert!(s.context().output.is_empty());

        // Announce is received, so subordinate sends it to computation.
        let s = s.process_announce(announce.clone()).unwrap();
        assert!(s.is_subordinate());
        assert_eq!(s.context().output, vec![announce.data().clone().into()]);

        // After announce is computed, not-validator subordinate switches to initial state.
        let s = s
            .process_computed_announce(announce.data().to_hash())
            .unwrap();
        assert!(s.is_initial());
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
            .mock_verified_data(producer, (block.hash, parent_announce_hash));
        let announce_alice = ctx
            .core
            .signer
            .mock_verified_data(alice, (block.hash, parent_announce_hash));

        ctx.pending(PendingEvent::Announce(announce_producer.clone()));
        ctx.pending(PendingEvent::Announce(announce_alice.clone()));

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true)
            .unwrap()
            .process_prepared_block(block.hash)
            .unwrap();
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
        let invalid_announce = ctx.core.signer.mock_verified_data(alice, block.hash);

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
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(());

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        let s = s.process_computed_announce(HashOf::random()).unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(s.context().output[0], ConsensusEvent::Warning(_)));
    }
}
