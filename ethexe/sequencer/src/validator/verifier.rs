use crate::{utils::BatchCommitmentValidationRequest, ControlError, ControlEvent};
use anyhow::anyhow;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_signer::{Address, SignedData};
use gprimitives::H256;

pub struct Verifier {
    producer: Address,
    block: SimpleBlockData,
    earlier_validation_request: Option<BatchCommitmentValidationRequest>,
    state: State,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq)]
enum State {
    WaitingParentComputed {
        parent_hash: H256,
    },
    WaitingForProducerBlock,
    WaitingProducerBlockComputed {
        // TODO +_+_+: change this to producer-block digest when off-chain transactions added
        block_hash: H256,
        parent_hash: Option<H256>,
    },
    Final,
}

impl Verifier {
    pub fn new(
        block: SimpleBlockData,
        producer: Address,
        received_producer_blocks: Vec<SignedData<ProducerBlock>>,
        received_validation_requests: Vec<SignedData<BatchCommitmentValidationRequest>>,
    ) -> Result<(Self, Vec<ControlEvent>), anyhow::Error> {
        let producer_block = received_producer_blocks.into_iter().find_map(|signed| {
            signed.verify_address(producer).ok().and_then(|_| {
                (signed.data().block_hash == block.hash).then_some(signed.into_parts().0)
            })
        });

        let earlier_validation_request =
            received_validation_requests.into_iter().find_map(|signed| {
                signed
                    .verify_address(producer)
                    .ok()
                    .map(|_| signed.into_parts().0)
            });

        let (state, events) = if let Some(pb) = producer_block {
            (
                State::WaitingProducerBlockComputed {
                    block_hash: block.hash,
                    parent_hash: None,
                },
                vec![ControlEvent::ComputeProducerBlock(pb)],
            )
        } else {
            let parent_hash = block.header.parent_hash;
            (
                State::WaitingParentComputed { parent_hash },
                vec![ControlEvent::ComputeBlock(parent_hash)],
            )
        };

        Ok((
            Self {
                producer,
                block,
                earlier_validation_request,
                state,
            },
            events,
        ))
    }

    pub fn receive_block_from_producer(
        &mut self,
        signed: SignedData<ProducerBlock>,
    ) -> Result<Vec<ControlEvent>, ControlError> {
        signed.verify_address(self.producer).map_err(|e| {
            ControlError::Warning(anyhow!("Received block is not signed by the producer: {e}"))
        })?;

        let (block, _) = signed.into_parts();

        let parent_hash_in_computation = match &self.state {
            State::WaitingParentComputed { parent_hash } => Some(*parent_hash),
            State::WaitingForProducerBlock => None,
            State::WaitingProducerBlockComputed { .. } | State::Final => {
                return Err(ControlError::Warning(anyhow!(
                    "Not waiting for producer block"
                )))
            }
        };

        self.state = State::WaitingProducerBlockComputed {
            block_hash: block.block_hash,
            parent_hash: parent_hash_in_computation,
        };

        Ok(vec![ControlEvent::ComputeProducerBlock(block)])
    }

    /// Returns whether the received block is a computed block from the producer
    pub fn receive_computed_block(&mut self, computed_block: H256) -> Result<bool, ControlError> {
        match &mut self.state {
            State::WaitingProducerBlockComputed {
                block_hash,
                parent_hash,
            } => {
                if computed_block == *block_hash {
                    self.state = State::Final;
                    Ok(true)
                } else if Some(computed_block) == *parent_hash {
                    Err(ControlError::EventSkipped)
                } else {
                    Err(ControlError::Warning(anyhow!(
                        "Received computed block {} is different from the expected block hash {}",
                        computed_block,
                        block_hash
                    )))
                }
            }
            State::WaitingParentComputed { parent_hash } => {
                if computed_block == *parent_hash {
                    self.state = State::WaitingForProducerBlock;
                    Ok(false)
                } else {
                    Err(ControlError::Warning(anyhow!(
                        "Received computed block {} is different from the expected parent hash {}",
                        computed_block,
                        parent_hash
                    )))
                }
            }
            State::WaitingForProducerBlock | State::Final => Err(ControlError::Warning(anyhow!(
                "Not waiting for computed block"
            ))),
        }
    }

    pub fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<(), ControlError> {
        request.verify_address(self.producer).map_err(|e| {
            ControlError::Warning(anyhow!(
                "Received validation request is not signed by the producer: {e}"
            ))
        })?;

        if self.earlier_validation_request.is_some() {
            return Err(ControlError::Warning(anyhow!(
                "Received second validation request"
            )));
        }

        self.earlier_validation_request = Some(request.into_parts().0);

        Ok(())
    }

    pub fn into_parts(
        self,
    ) -> (
        Address,
        SimpleBlockData,
        Option<BatchCommitmentValidationRequest>,
    ) {
        if !matches!(self.state, State::Final) {
            unreachable!("Verifier is not in final state: invalid verifier usage")
        }

        (self.producer, self.block, self.earlier_validation_request)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    #[test]
    fn new_empty() {
        let (_, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];

        let data = mock_simple_block_data();

        let (verifier, events) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        assert_eq!(verifier.producer, producer.to_address());
        assert_eq!(verifier.block, data);
        assert!(verifier.earlier_validation_request.is_none());
        assert_eq!(
            verifier.state,
            State::WaitingParentComputed {
                parent_hash: data.header.parent_hash
            }
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].clone(),
            ControlEvent::ComputeBlock(hash) if hash == data.header.parent_hash
        ));
    }

    #[test]
    fn new_with_producer_blocks() {
        let (signer, _, pub_keys) = init_signer_with_keys(2);

        let producer = pub_keys[0];
        let alice = pub_keys[1];

        let data = mock_simple_block_data();

        let (_, invalid_signed_pb) = mock_producer_block(&signer, alice, data.hash);
        let (verifier, _) = Verifier::new(
            data.clone(),
            producer.to_address(),
            vec![invalid_signed_pb.clone()],
            vec![],
        )
        .unwrap();

        assert!(matches!(
            verifier.state,
            State::WaitingParentComputed { .. }
        ));

        let (pb, signed_pb) = mock_producer_block(&signer, producer, data.hash);

        let (verifier, events) = Verifier::new(
            data.clone(),
            producer.to_address(),
            vec![invalid_signed_pb, signed_pb],
            vec![],
        )
        .unwrap();

        assert_eq!(verifier.producer, producer.to_address());
        assert_eq!(verifier.block, data);
        assert!(verifier.earlier_validation_request.is_none());
        assert_eq!(
            verifier.state,
            State::WaitingProducerBlockComputed {
                block_hash: data.hash,
                parent_hash: None
            }
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].clone(),
            ControlEvent::ComputeProducerBlock(block) if block == pb
        ));
    }

    #[test]
    fn new_with_validation_requests() {
        let (signer, _, pub_keys) = init_signer_with_keys(2);

        let producer = pub_keys[0];
        let alice = pub_keys[1];

        let data = mock_simple_block_data();

        let request = BatchCommitmentValidationRequest {
            blocks: vec![],
            codes: vec![],
        };

        let invalid_signed_request = signer.create_signed_data(alice, request.clone()).unwrap();

        let (verifier, _) = Verifier::new(
            data.clone(),
            producer.to_address(),
            vec![],
            vec![invalid_signed_request.clone()],
        )
        .unwrap();

        assert!(verifier.earlier_validation_request.is_none());

        let signed_request = signer
            .create_signed_data(producer, request.clone())
            .unwrap();

        let (verifier, events) = Verifier::new(
            data.clone(),
            producer.to_address(),
            vec![],
            vec![invalid_signed_request, signed_request],
        )
        .unwrap();

        assert_eq!(verifier.producer, producer.to_address());
        assert_eq!(verifier.block, data);
        assert!(verifier.earlier_validation_request.is_some());
        assert_eq!(
            verifier.state,
            State::WaitingParentComputed {
                parent_hash: data.header.parent_hash
            }
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].clone(),
            ControlEvent::ComputeBlock(hash) if hash == data.header.parent_hash
        ));
    }

    #[test]
    fn receive_block_from_producer() {
        let (signer, _, pub_keys) = init_signer_with_keys(2);

        let producer = pub_keys[0];
        let alice = pub_keys[1];

        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        let (_, invalid_signed_pb) = mock_producer_block(&signer, alice, data.hash);
        let result = verifier.receive_block_from_producer(invalid_signed_pb);
        assert!(matches!(result, Err(ControlError::Warning(_))));
        assert!(matches!(
            verifier.state,
            State::WaitingParentComputed { .. }
        ));

        let (pb, signed_pb) = mock_producer_block(&signer, producer, data.hash);
        let events = verifier
            .receive_block_from_producer(signed_pb.clone())
            .unwrap();

        assert_eq!(
            verifier.state,
            State::WaitingProducerBlockComputed {
                block_hash: data.hash,
                parent_hash: Some(data.header.parent_hash)
            }
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].clone(),
            ControlEvent::ComputeProducerBlock(block) if block == pb
        ));

        let res = verifier.receive_block_from_producer(signed_pb);
        assert!(matches!(res, Err(ControlError::Warning(_))));
        assert!(matches!(
            verifier.state,
            State::WaitingProducerBlockComputed { .. }
        ));
    }

    #[test]
    fn receive_computed_block() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];
        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        let parent_hash = data.header.parent_hash;

        // Test receiving a computed block matching the parent hash
        let result = verifier.receive_computed_block(parent_hash);
        assert!(matches!(result, Ok(false)));
        assert!(matches!(verifier.state, State::WaitingForProducerBlock));

        // Test receiving a computed block matching the block hash
        let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
        verifier.receive_block_from_producer(signed_pb).unwrap();

        let result = verifier.receive_computed_block(data.hash);
        assert!(matches!(result, Ok(true)));
        assert!(matches!(verifier.state, State::Final));

        // Test receiving an invalid computed block
        let invalid_hash = H256::random();
        let result = verifier.receive_computed_block(invalid_hash);
        assert!(matches!(result, Err(ControlError::Warning(_))));
    }

    #[test]
    fn receive_validation_request() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];
        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        let (_, signed_request) = mock_validation_request(&signer, producer);

        // Test receiving a valid validation request
        verifier
            .receive_validation_request(signed_request.clone())
            .unwrap();
        assert!(verifier.earlier_validation_request.is_some());

        // Test receiving a second validation request
        let result = verifier.receive_validation_request(signed_request);
        assert!(matches!(result, Err(ControlError::Warning(_))));
    }

    #[test]
    fn reject_unknown_signature_validation_request() {
        let (signer, _, pub_keys) = init_signer_with_keys(2);

        let producer = pub_keys[0];
        let alice = pub_keys[1];

        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        let (_, signed_request) = mock_validation_request(&signer, alice);

        // Test receiving a validation request with an unknown signature (signed by Alice)
        let result = verifier.receive_validation_request(signed_request);
        assert!(matches!(result, Err(ControlError::Warning(_))));
        assert!(verifier.earlier_validation_request.is_none());
    }

    #[test]
    fn into_parts() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];
        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        // Move verifier to final state
        let parent_hash = data.header.parent_hash;
        verifier.receive_computed_block(parent_hash).unwrap();

        let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
        verifier.receive_block_from_producer(signed_pb).unwrap();
        verifier.receive_computed_block(data.hash).unwrap();

        // Test into_parts
        let (returned_producer, returned_block, returned_request) = verifier.into_parts();
        assert_eq!(returned_producer, producer.to_address());
        assert_eq!(returned_block, data);
        assert!(returned_request.is_none());
    }

    #[test]
    fn invalid_state_transitions() {
        let (signer, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];
        let data = mock_simple_block_data();

        let (mut verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        // Test receiving a producer block in an invalid state
        let (_, signed_pb) = mock_producer_block(&signer, producer, data.hash);
        verifier.state = State::Final;
        let result = verifier.receive_block_from_producer(signed_pb);
        assert!(matches!(result, Err(ControlError::Warning(_))));

        // Test receiving a computed block in an invalid state
        verifier.state = State::Final;
        let result = verifier.receive_computed_block(data.hash);
        assert!(matches!(result, Err(ControlError::Warning(_))));
    }

    #[test]
    #[should_panic(expected = "Verifier is not in final state: invalid verifier usage")]
    fn into_parts_panics_if_not_final() {
        let (_, _, pub_keys) = init_signer_with_keys(1);

        let producer = pub_keys[0];
        let data = mock_simple_block_data();

        let (verifier, _) =
            Verifier::new(data.clone(), producer.to_address(), vec![], vec![]).unwrap();

        // Attempt to call into_parts while the state is not final
        let _ = verifier.into_parts();
    }
}
