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
            _ => {
                return Err(ControlError::Warning(anyhow!(
                    "Received not waited producer block"
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
            _ => Err(ControlError::Warning(anyhow!(
                "Received computed block in unexpected state"
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
        (self.producer, self.block, self.earlier_validation_request)
    }
}
