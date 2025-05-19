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

use super::{initial::Initial, DefaultProcessing, PendingEvent, StateHandler, ValidatorContext};
use crate::{validator::participant::Participant, ConsensusEvent};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{ecdsa::SignedData, Address, ProducerBlock, SimpleBlockData};
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

#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingForProducerBlock,
    WaitingProducerBlockComputed {
        // TODO #4640: change this to producer-block digest when off-chain transactions added
        block_hash: H256,
    },
}

impl StateHandler for Subordinate {
    fn into_dyn(self: Box<Self>) -> Box<dyn StateHandler> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut super::ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_block_from_producer(
        mut self: Box<Self>,
        block: SignedData<ProducerBlock>,
    ) -> Result<Box<dyn StateHandler>> {
        if self.state == State::WaitingForProducerBlock
            && block.address() == self.producer
            && block.data().block_hash == self.block.hash
        {
            let pb = block.into_parts().0;
            let block_hash = pb.block_hash;

            self.output(ConsensusEvent::ComputeProducerBlock(pb));

            self.state = State::WaitingProducerBlockComputed { block_hash };

            Ok(self)
        } else {
            DefaultProcessing::block_from_producer(self, block)
        }
    }

    fn process_validation_request(
        mut self: Box<Self>,
        request: SignedData<crate::BatchCommitmentValidationRequest>,
    ) -> Result<Box<dyn StateHandler>> {
        if request.address() == self.producer {
            log::trace!("Receive validation request from producer: {request:?}, saved for later.");
            self.ctx.pending(request);

            Ok(self)
        } else {
            DefaultProcessing::validation_request(self, request)
        }
    }

    fn process_computed_block(
        self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn StateHandler>> {
        match &self.state {
            _ if computed_block == self.block.header.parent_hash => {
                // Earlier we sent a task for parent block computation.
                // Continue to wait for block from producer.
                Ok(self)
            }
            State::WaitingProducerBlockComputed { block_hash } if computed_block == *block_hash => {
                if self.is_validator {
                    Participant::create(self.ctx, self.block, self.producer)
                } else {
                    Initial::create(self.ctx)
                }
            }
            _ => DefaultProcessing::computed_block(self, computed_block),
        }
    }
}

impl Subordinate {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
        is_validator: bool,
    ) -> Result<Box<dyn StateHandler>> {
        let mut earlier_producer_block = None;

        // Search for already received producer blocks.
        // If events amount is eq to MAX_PENDING_EVENTS, then oldest ones would be removed.
        // TODO #4641: potential abuse can be here. If we receive a lot of fake events,
        // important ones can be removed. What to do:
        // 1) Check event is sent by current or next or previous era validator.
        // 2) Malicious validator can send a lot of events (consider what to do).
        for event in mem::take(&mut ctx.pending_events) {
            match event {
                PendingEvent::ProducerBlock(signed_pb)
                    if earlier_producer_block.is_none()
                        && (signed_pb.data().block_hash == block.hash)
                        && signed_pb.address() == producer =>
                {
                    earlier_producer_block = Some(signed_pb.into_parts().0);
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

        let state = if let Some(producer_block) = earlier_producer_block {
            let block_hash = producer_block.block_hash;
            ctx.output(ConsensusEvent::ComputeProducerBlock(producer_block));

            State::WaitingProducerBlockComputed { block_hash }
        } else {
            ctx.output(ConsensusEvent::ComputeBlock(block.header.parent_hash));

            State::WaitingForProducerBlock
        };

        Ok(Box::new(Self {
            ctx,
            producer,
            block,
            is_validator,
            state,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::mock_validator_context};
    use std::any::TypeId;

    #[test]
    fn create_empty() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeBlock(block.header.parent_hash)]
        );
        assert_eq!(s.context().pending_events, vec![]);
    }

    #[test]
    fn create_with_producer_blocks() {
        let (mut ctx, keys) = mock_validator_context();
        let producer = keys[0];
        let block = mock_simple_block_data();
        let (pb1, signed_pb1) = mock_producer_block(&ctx.signer, producer, block.hash);
        let (_, signed_pb2) = mock_producer_block(&ctx.signer, keys[1], block.hash);

        ctx.pending(signed_pb1);
        ctx.pending(signed_pb2.clone());

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeProducerBlock(pb1)]
        );

        // Second block must stay in pending events, because it's not from current producer.
        assert_eq!(
            s.context().pending_events,
            vec![PendingEvent::ProducerBlock(signed_pb2)]
        );
    }

    #[test]
    fn create_with_validation_requests() {
        let (mut ctx, keys) = mock_validator_context();
        let producer = keys[0];
        let block = mock_simple_block_data();
        let (_, signed_request1) = mock_validation_request(&ctx.signer, producer);
        let (_, signed_request2) = mock_validation_request(&ctx.signer, keys[1]);

        ctx.pending(signed_request1.clone());
        ctx.pending(signed_request2.clone());

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeBlock(block.header.parent_hash)]
        );
        assert_eq!(
            s.context().pending_events,
            vec![
                PendingEvent::ValidationRequest(signed_request2),
                PendingEvent::ValidationRequest(signed_request1)
            ]
        );
    }

    #[test]
    fn create_with_many_pending_events() {
        let (mut ctx, keys) = mock_validator_context();
        let producer = keys[0];
        let block = mock_simple_block_data();
        let (pb, signed_pb) = mock_producer_block(&ctx.signer, producer, block.hash);

        ctx.pending(signed_pb);

        // Fill with fake blocks
        for _ in 0..10 * MAX_PENDING_EVENTS {
            ctx.pending(mock_producer_block(&ctx.signer, keys[1], block.hash).1);
        }

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeProducerBlock(pb)]
        );
        assert_eq!(s.context().pending_events.len(), MAX_PENDING_EVENTS);
    }

    #[test]
    fn simple() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (pb, signed_pb) = mock_producer_block(&ctx.signer, producer, block.hash);

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 1);
        assert_eq!(
            s.context().output[0],
            ConsensusEvent::ComputeBlock(block.header.parent_hash)
        );

        let s = s.process_block_from_producer(signed_pb).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);
        assert_eq!(
            s.context().output[1],
            ConsensusEvent::ComputeProducerBlock(pb)
        );

        let s = s.process_computed_block(block.header.parent_hash).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);

        let s = s.process_computed_block(block.hash).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Participant>());
        assert_eq!(s.context().output.len(), 2);
    }

    #[test]
    fn simple_not_validator() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (pb, signed_pb) = mock_producer_block(&ctx.signer, producer, block.hash);

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), false).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 1);
        assert_eq!(
            s.context().output[0],
            ConsensusEvent::ComputeBlock(block.header.parent_hash)
        );

        let s = s.process_block_from_producer(signed_pb).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);
        assert_eq!(
            s.context().output[1],
            ConsensusEvent::ComputeProducerBlock(pb)
        );

        let s = s.process_computed_block(block.header.parent_hash).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);

        let s = s.process_computed_block(block.hash).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Initial>());
    }

    #[test]
    fn create_with_multiple_producer_blocks() {
        let (mut ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (pb1, signed_pb1) = mock_producer_block(&ctx.signer, producer, block.hash);
        let (_, signed_pb2) = mock_producer_block(&ctx.signer, producer, block.hash);

        ctx.pending(signed_pb1);
        ctx.pending(signed_pb2.clone());

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        assert_eq!(
            s.context().output,
            vec![ConsensusEvent::ComputeProducerBlock(pb1)]
        );
        assert_eq!(
            s.context().pending_events,
            vec![PendingEvent::ProducerBlock(signed_pb2)]
        );
    }

    #[test]
    fn process_external_event_with_invalid_producer_block() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, invalid_pb) = mock_producer_block(&ctx.signer, pub_keys[1], block.hash);

        let mut s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        s = s.process_block_from_producer(invalid_pb.clone()).unwrap();
        assert_eq!(s.context().output.len(), 2);
        assert!(matches!(s.context().output[1], ConsensusEvent::Warning(_)));
        assert_eq!(s.context().pending_events.len(), 1);
        assert_eq!(s.context().pending_events[0], invalid_pb.into());
    }

    #[test]
    fn process_computed_block_with_unexpected_hash() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let unexpected_hash = H256::random();

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        let s = s.process_computed_block(unexpected_hash).unwrap();
        assert_eq!(s.context().output.len(), 2);
        assert!(matches!(s.context().output[1], ConsensusEvent::Warning(_)));
    }
}
