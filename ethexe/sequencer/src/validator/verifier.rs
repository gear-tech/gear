use anyhow::Result;
use ethexe_common::SimpleBlockData;
use ethexe_signer::Address;
use gprimitives::H256;
use std::mem;

use super::{ExternalEvent, ValidatorContext, ValidatorSubService};
use crate::{validator::participant::Participant, ControlEvent};

pub struct Verifier {
    ctx: ValidatorContext,
    producer: Address,
    block: SimpleBlockData,
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

impl ValidatorSubService for Verifier {
    fn log(&self, s: String) -> String {
        format!("VERIFIER in {state:?} - {s}", state = self.state)
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
                self.ctx
                    .output
                    .push_back(ControlEvent::ComputeProducerBlock(pb.into_parts().0));

                Ok(self)
            }
            (_, event @ ExternalEvent::ValidationRequest(_)) => {
                self.ctx.pending_events.push_back(event);

                Ok(self)
            }
            (_, event) => {
                self.ctx
                    .warning(self.log(format!("unexpected event: {event:?}, saved for later")));

                self.ctx.pending_events.push_back(event);

                Ok(self)
            }
        }
    }

    fn process_computed_block(
        mut self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        if computed_block == self.block.header.parent_hash {
            // Earlier we sent a task for parent block computation.
            // Continue to wait for block from producer.

            return Ok(self);
        }

        if matches!(&self.state, State::WaitingProducerBlockComputed { block_hash, .. } if computed_block == *block_hash)
        {
            return Participant::create(self.ctx, self.block, self.producer);
        }

        self.ctx
            .warning(self.log(format!("unexpected computed block: {computed_block:?}")));

        Ok(self)
    }
}

impl Verifier {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
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
                event @ ExternalEvent::ValidationRequest(_) => {
                    ctx.pending_events.push_back(event);
                }
                _ => {
                    // NOTE: skip other events
                }
            }
        }

        let state = if let Some(producer_block) = earlier_producer_block {
            let block_hash = producer_block.block_hash;
            ctx.output
                .push_back(ControlEvent::ComputeProducerBlock(producer_block));
            State::WaitingProducerBlockComputed { block_hash }
        } else {
            ctx.output
                .push_back(ControlEvent::ComputeBlock(block.header.parent_hash));
            State::WaitingForProducerBlock
        };

        Ok(Box::new(Self {
            ctx,
            producer,
            block,
            state,
        }))
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::test_utils::*;

//     #[test]
//     fn new_empty() {
//         let (_, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];

//         let data = mock_simple_block_data();

//         let (verifier, events) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         assert_eq!(verifier.producer, producer.to_address());
//         assert_eq!(verifier.block, data);
//         assert!(verifier.earlier_validation_request.is_none());
//         assert_eq!(
//             verifier.state,
//             State::WaitingParentComputed {
//                 parent_hash: data.header.parent_hash
//             }
//         );

//         assert_eq!(events.len(), 1);
//         assert!(matches!(
//             events[0].clone(),
//             ControlEvent::ComputeBlock(hash) if hash == data.header.parent_hash
//         ));
//     }

//     #[test]
//     fn new_with_producer_blocks() {
//         let (signer, _, pub_keys) = init_signer_with_keys(2);

//         let producer = pub_keys[0];
//         let alice = pub_keys[1];

//         let data = mock_simple_block_data();

//         let (_, invalid_signed_pb) = mock_producer_block(&signer, alice, data.hash);
//         let (verifier, _) = Verifier::new(
//             data.clone(),
//             producer.to_address(),
//             vec![invalid_signed_pb.clone()],
//             vec![],
//         )
//         .unwrap();

//         assert!(matches!(
//             verifier.state,
//             State::WaitingParentComputed { .. }
//         ));

//         let (pb, signed_pb) = mock_producer_block(&signer, producer, data.hash);

//         let (verifier, events) = Verifier::new(
//             data.clone(),
//             producer.to_address(),
//             vec![invalid_signed_pb, signed_pb],
//             vec![],
//         )
//         .unwrap();

//         assert_eq!(verifier.producer, producer.to_address());
//         assert_eq!(verifier.block, data);
//         assert!(verifier.earlier_validation_request.is_none());
//         assert_eq!(
//             verifier.state,
//             State::WaitingProducerBlockComputed {
//                 block_hash: data.hash,
//                 parent_hash: None
//             }
//         );

//         assert_eq!(events.len(), 1);
//         assert!(matches!(
//             events[0].clone(),
//             ControlEvent::ComputeProducerBlock(block) if block == pb
//         ));
//     }

//     #[test]
//     fn new_with_validation_requests() {
//         let (signer, _, pub_keys) = init_signer_with_keys(2);

//         let producer = pub_keys[0];
//         let alice = pub_keys[1];

//         let data = mock_simple_block_data();

//         let request = BatchCommitmentValidationRequest {
//             blocks: vec![],
//             codes: vec![],
//         };

//         let invalid_signed_request = signer.create_signed_data(alice, request.clone()).unwrap();

//         let (verifier, _) = Verifier::new(
//             data.clone(),
//             producer.to_address(),
//             vec![],
//             vec![invalid_signed_request.clone()],
//         )
//         .unwrap();

//         assert!(verifier.earlier_validation_request.is_none());

//         let signed_request = signer
//             .create_signed_data(producer, request.clone())
//             .unwrap();

//         let (verifier, events) = Verifier::new(
//             data.clone(),
//             producer.to_address(),
//             vec![],
//             vec![invalid_signed_request, signed_request],
//         )
//         .unwrap();

//         assert_eq!(verifier.producer, producer.to_address());
//         assert_eq!(verifier.block, data);
//         assert!(verifier.earlier_validation_request.is_some());
//         assert_eq!(
//             verifier.state,
//             State::WaitingParentComputed {
//                 parent_hash: data.header.parent_hash
//             }
//         );

//         assert_eq!(events.len(), 1);
//         assert!(matches!(
//             events[0].clone(),
//             ControlEvent::ComputeBlock(hash) if hash == data.header.parent_hash
//         ));
//     }

//     #[test]
//     fn receive_block_from_producer() {
//         let (signer, _, pub_keys) = init_signer_with_keys(2);

//         let producer = pub_keys[0];
//         let alice = pub_keys[1];

//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         let (_, invalid_signed_pb) = mock_producer_block(&signer, alice, data.hash);
//         let result = verifier.receive_block_from_producer(invalid_signed_pb);
//         assert!(matches!(result, Err(ControlError::Warning(_))));
//         assert!(matches!(
//             verifier.state,
//             State::WaitingParentComputed { .. }
//         ));

//         let (pb, signed_pb) = mock_producer_block(&signer, producer, data.hash);
//         let events = verifier
//             .receive_block_from_producer(signed_pb.clone())
//             .unwrap();

//         assert_eq!(
//             verifier.state,
//             State::WaitingProducerBlockComputed {
//                 block_hash: data.hash,
//                 parent_hash: Some(data.header.parent_hash)
//             }
//         );

//         assert_eq!(events.len(), 1);
//         assert!(matches!(
//             events[0].clone(),
//             ControlEvent::ComputeProducerBlock(block) if block == pb
//         ));

//         let res = verifier.receive_block_from_producer(signed_pb);
//         assert!(matches!(res, Err(ControlError::Warning(_))));
//         assert!(matches!(
//             verifier.state,
//             State::WaitingProducerBlockComputed { .. }
//         ));
//     }

//     #[test]
//     fn receive_computed_block() {
//         let (signer, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];
//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         let parent_hash = data.header.parent_hash;

//         // Test receiving a computed block matching the parent hash
//         let result = verifier.receive_computed_block(parent_hash);
//         assert!(matches!(result, Ok(false)));
//         assert!(matches!(verifier.state, State::WaitingForProducerBlock));

//         // Test receiving a computed block matching the block hash
//         let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
//         verifier.receive_block_from_producer(signed_pb).unwrap();

//         let result = verifier.receive_computed_block(data.hash);
//         assert!(matches!(result, Ok(true)));
//         assert!(matches!(verifier.state, State::Final));

//         // Test receiving an invalid computed block
//         let invalid_hash = H256::random();
//         let result = verifier.receive_computed_block(invalid_hash);
//         assert!(matches!(result, Err(ControlError::Warning(_))));
//     }

//     #[test]
//     fn receive_validation_request() {
//         let (signer, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];
//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         let (_, signed_request) = mock_validation_request(&signer, producer);

//         // Test receiving a valid validation request
//         verifier
//             .receive_validation_request(signed_request.clone())
//             .unwrap();
//         assert!(verifier.earlier_validation_request.is_some());

//         // Test receiving a second validation request
//         let result = verifier.receive_validation_request(signed_request);
//         assert!(matches!(result, Err(ControlError::Warning(_))));
//     }

//     #[test]
//     fn reject_unknown_signature_validation_request() {
//         let (signer, _, pub_keys) = init_signer_with_keys(2);

//         let producer = pub_keys[0];
//         let alice = pub_keys[1];

//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         let (_, signed_request) = mock_validation_request(&signer, alice);

//         // Test receiving a validation request with an unknown signature (signed by Alice)
//         let result = verifier.receive_validation_request(signed_request);
//         assert!(matches!(result, Err(ControlError::Warning(_))));
//         assert!(verifier.earlier_validation_request.is_none());
//     }

//     #[test]
//     fn into_parts() {
//         let (signer, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];
//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         // Move verifier to final state
//         let parent_hash = data.header.parent_hash;
//         verifier.receive_computed_block(parent_hash).unwrap();

//         let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
//         verifier.receive_block_from_producer(signed_pb).unwrap();
//         verifier.receive_computed_block(data.hash).unwrap();

//         // Test into_parts
//         let (returned_producer, returned_block, returned_request) = verifier.into_parts();
//         assert_eq!(returned_producer, producer.to_address());
//         assert_eq!(returned_block, data);
//         assert!(returned_request.is_none());
//     }

//     #[test]
//     fn invalid_state_transitions() {
//         let (signer, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];
//         let data = mock_simple_block_data();

//         let (mut verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         // Test receiving a producer block in an invalid state
//         let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
//         verifier.state = State::Final;
//         let result = verifier.receive_block_from_producer(signed_pb);
//         assert!(matches!(result, Err(ControlError::Warning(_))));

//         // Test receiving a computed block in an invalid state
//         verifier.state = State::Final;
//         let result = verifier.receive_computed_block(data.hash);
//         assert!(matches!(result, Err(ControlError::Warning(_))));
//     }

//     #[test]
//     #[should_panic(expected = "Verifier is not in final state: invalid verifier usage")]
//     fn into_parts_panics_if_not_final() {
//         let (_, _, pub_keys) = init_signer_with_keys(1);

//         let producer = pub_keys[0];
//         let data = mock_simple_block_data();

//         let (verifier, _) =
//             Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

//         // Attempt to call into_parts while the state is not final
//         let _ = verifier.into_parts();
//     }
// }
