use anyhow::Result;
use ethexe_common::SimpleBlockData;
use ethexe_signer::Address;
use gprimitives::H256;
use std::mem;

use super::{initial::Initial, ExternalEvent, ValidatorContext, ValidatorSubService};
use crate::{validator::participant::Participant, ControlEvent};

pub struct Subordinate {
    ctx: ValidatorContext,
    producer: Address,
    block: SimpleBlockData,
    // +_+_+ test this mode when false
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
    fn log(&self, s: String) -> String {
        format!("SUBORDINATE in {state:?} - {s}", state = self.state)
    }

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

    fn process_external_event(
        mut self: Box<Self>,
        event: ExternalEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match (&self.state, event) {
            (State::WaitingForProducerBlock, ExternalEvent::ProducerBlock(pb))
                if pb.verify_address(self.producer).is_ok()
                    && (pb.data().block_hash == self.block.hash) =>
            {
                let pb = pb.into_parts().0;
                let block_hash = pb.block_hash;

                self.output(ControlEvent::ComputeProducerBlock(pb));

                self.state = State::WaitingProducerBlockComputed { block_hash };

                Ok(self)
            }
            (_, ExternalEvent::ValidationRequest(request))
                if request.verify_address(self.producer).is_ok() =>
            {
                if self.is_validator {
                    log::trace!(
                        "Receive validation request from producer: {request:?}, saved for later"
                    );
                    self.ctx.pending_events.push_back(request.into());
                }

                Ok(self)
            }
            (_, event) => super::process_external_event_by_default(self, event),
        }
    }

    fn process_computed_block(
        mut self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match &self.state {
            _ if computed_block == self.block.header.parent_hash => {
                // Earlier we sent a task for parent block computation.
                // Continue to wait for block from producer.
                Ok(self)
            }
            State::WaitingProducerBlockComputed { block_hash, .. }
                if computed_block == *block_hash =>
            {
                if self.is_validator {
                    Participant::create(self.ctx, self.block, self.producer)
                } else {
                    Initial::create(self.ctx)
                }
            }
            _ => {
                self.warning(format!("unexpected computed block: {computed_block:?}"));

                Ok(self)
            }
        }
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
        let pending_events = mem::take(&mut ctx.pending_events);
        for event in pending_events {
            match event {
                ExternalEvent::ProducerBlock(signed_data)
                    if earlier_producer_block.is_none()
                        && (signed_data.data().block_hash == block.hash)
                        && signed_data.verify_address(producer).is_ok() =>
                {
                    earlier_producer_block = Some(signed_data.into_parts().0);
                }
                event @ ExternalEvent::ValidationRequest(_) if is_validator => {
                    // Keep validation requests if we are a validator
                    ctx.pending_events.push_back(event);
                }
                _ => {}
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
    use super::*;
    use crate::{test_utils::*, validator::tests::mock_validator_context};

    #[test]
    fn create_empty() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(
            s.context().output,
            vec![ControlEvent::ComputeBlock(block.header.parent_hash)]
        );
        assert_eq!(s.context().pending_events.len(), 0);
    }

    #[test]
    fn create_with_producer_blocks() {
        let (mut ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (pb, signed_pb) = mock_producer_block(&ctx.signer, producer, block.hash);

        ctx.pending_events
            .push_back(ExternalEvent::ProducerBlock(signed_pb));

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        assert_eq!(
            s.context().output,
            vec![ControlEvent::ComputeProducerBlock(pb)]
        );
        assert_eq!(s.context().pending_events.len(), 0);
    }

    #[test]
    fn create_with_validation_requests() {
        let (mut ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, signed_request) = mock_validation_request(&ctx.signer, producer);

        ctx.pending_events
            .push_back(ExternalEvent::ValidationRequest(signed_request.clone()));

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        assert_eq!(
            s.context().output,
            vec![ControlEvent::ComputeBlock(block.header.parent_hash)]
        );
        assert_eq!(s.context().pending_events.len(), 1,);
        assert_eq!(
            s.context().pending_events[0],
            ExternalEvent::ValidationRequest(signed_request)
        );

        let ctx = s.into_context();
        let s = Subordinate::create(ctx, block, producer.to_address(), false).unwrap();
        assert_eq!(s.context().pending_events.len(), 0);
    }

    #[test]
    fn simple() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (pb, signed_pb) = mock_producer_block(&ctx.signer, producer, block.hash);

        let s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();
        assert_eq!(s.context().output.len(), 1);
        assert_eq!(
            s.context().output[0],
            ControlEvent::ComputeBlock(block.header.parent_hash)
        );

        let s = s
            .process_external_event(ExternalEvent::ProducerBlock(signed_pb))
            .unwrap();
        assert_eq!(s.context().output.len(), 2);
        assert_eq!(
            s.context().output[1],
            ControlEvent::ComputeProducerBlock(pb)
        );

        let s = s.process_computed_block(block.header.parent_hash).unwrap();
        assert_eq!(s.context().output.len(), 2);

        let s = s.process_computed_block(block.hash).unwrap();
        assert_eq!(s.context().output.len(), 2);
    }

    #[test]
    fn create_with_multiple_producer_blocks() {
        let (mut ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, signed_pb1) = mock_producer_block(&ctx.signer, producer, block.hash);
        let (_, signed_pb2) = mock_producer_block(&ctx.signer, producer, block.hash);

        ctx.pending_events
            .push_back(ExternalEvent::ProducerBlock(signed_pb1));
        ctx.pending_events
            .push_back(ExternalEvent::ProducerBlock(signed_pb2));

        let s = Subordinate::create(ctx, block, producer.to_address(), true).unwrap();

        assert_eq!(s.context().output.len(), 1);
        assert!(matches!(
            s.context().output[0],
            ControlEvent::ComputeProducerBlock(_)
        ));
        assert_eq!(s.context().pending_events.len(), 0);
    }

    #[test]
    fn process_external_event_with_invalid_producer_block() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, signed_pb) = mock_producer_block(&ctx.signer, pub_keys[1], block.hash);

        let mut s = Subordinate::create(ctx, block.clone(), producer.to_address(), true).unwrap();

        let invalid_pb = ExternalEvent::ProducerBlock(signed_pb);
        s = s.process_external_event(invalid_pb.clone()).unwrap();
        assert_eq!(s.context().output.len(), 2);
        assert!(matches!(s.context().output[1], ControlEvent::Warning(_)));
        assert_eq!(s.context().pending_events.len(), 1);
        assert_eq!(s.context().pending_events[0], invalid_pb);
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

    #[test]
    fn process_validation_request_as_non_validator() {
        let (ctx, pub_keys) = mock_validator_context();
        let producer = pub_keys[0];
        let block = mock_simple_block_data();
        let (_, signed_request) = mock_validation_request(&ctx.signer, producer);

        let mut s = Subordinate::create(ctx, block, producer.to_address(), false).unwrap();

        s = s
            .process_external_event(ExternalEvent::ValidationRequest(signed_request))
            .unwrap();

        assert_eq!(s.context().pending_events.len(), 0);
    }
}
