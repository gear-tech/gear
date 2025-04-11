use anyhow::Result;
use core::fmt;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_signer::{Address, SignedData};
use gprimitives::H256;
use std::mem;

use super::{
    initial::Initial, DefaultProcessing, PendingEvent, ValidatorContext, ValidatorSubService,
};
use crate::{validator::participant::Participant, ControlEvent};

const MAX_PENDING_EVENTS: usize = 10;

#[derive(Debug)]
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
        // TODO +_+_+: change this to producer-block digest when off-chain transactions added
        block_hash: H256,
    },
}

impl ValidatorSubService for Subordinate {
    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
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
    ) -> Result<Box<dyn ValidatorSubService>> {
        if self.state == State::WaitingForProducerBlock
            && block.verify_address(self.producer).is_ok()
            && block.data().block_hash == self.block.hash
        {
            let pb = block.into_parts().0;
            let block_hash = pb.block_hash;

            self.output(ControlEvent::ComputeProducerBlock(pb));

            self.state = State::WaitingProducerBlockComputed { block_hash };

            Ok(self)
        } else {
            DefaultProcessing::block_from_producer(self, block)
        }
    }

    fn process_validation_request(
        mut self: Box<Self>,
        request: SignedData<crate::BatchCommitmentValidationRequest>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        if request.verify_address(self.producer).is_ok() {
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
    ) -> Result<Box<dyn ValidatorSubService>> {
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

impl fmt::Display for Subordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("SUBORDINATE in {:?}", self.state))
    }
}

impl Subordinate {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
        is_validator: bool,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let mut earlier_producer_block = None;

        // Search for already received producer blocks.
        // If events amount is eq to MAX_PENDING_EVENTS, then oldest ones would be removed.
        // TODO: potential abuse can be here. If we receive a lot of fake events,
        // important ones can be removed. What to do:
        // 1) Check event is sent by current or next or previous era validator.
        // 2) Malicious validator can send a lot of events (consider what to do).
        for event in mem::take(&mut ctx.pending_events) {
            match event {
                PendingEvent::ProducerBlock(signed_pb)
                    if earlier_producer_block.is_none()
                        && (signed_pb.data().block_hash == block.hash)
                        && signed_pb.verify_address(producer).is_ok() =>
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
            ctx.output(ControlEvent::ComputeProducerBlock(producer_block));

            State::WaitingProducerBlockComputed { block_hash }
        } else {
            ctx.output(ControlEvent::ComputeBlock(block.header.parent_hash));

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
    use std::any::TypeId;

    use super::*;
    use crate::{tests::*, validator::tests::mock_validator_context};

    #[test]
    fn create_empty() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(
            s.context().output,
            vec![ControlEvent::ComputeBlock(block.header.parent_hash)]
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
            vec![ControlEvent::ComputeProducerBlock(pb1)]
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
            vec![ControlEvent::ComputeBlock(block.header.parent_hash)]
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
            vec![ControlEvent::ComputeProducerBlock(pb)]
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
            ControlEvent::ComputeBlock(block.header.parent_hash)
        );

        let s = s.process_block_from_producer(signed_pb).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);
        assert_eq!(
            s.context().output[1],
            ControlEvent::ComputeProducerBlock(pb)
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
            ControlEvent::ComputeBlock(block.header.parent_hash)
        );

        let s = s.process_block_from_producer(signed_pb).unwrap();
        assert_eq!(s.type_id(), TypeId::of::<Subordinate>());
        assert_eq!(s.context().output.len(), 2);
        assert_eq!(
            s.context().output[1],
            ControlEvent::ComputeProducerBlock(pb)
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
            vec![ControlEvent::ComputeProducerBlock(pb1)]
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
        assert!(matches!(s.context().output[1], ControlEvent::Warning(_)));
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
        assert!(matches!(s.context().output[1], ControlEvent::Warning(_)));
    }
}
