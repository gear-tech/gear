use anyhow::anyhow;
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_signer::{Address, SignedData};
use gprimitives::H256;

use crate::{
    bp::{ControlError, ControlEvent},
    utils::BatchCommitmentValidationRequest,
};

pub struct Verifier {
    producer: Address,
    block: SimpleBlockData,
    earlier_validation_request: Option<BatchCommitmentValidationRequest>,
    state: VerifierState,
}

enum VerifierState {
    WaitingParentComputed {
        parent_hash: H256,
    },
    WaitingForProducerBlock,
    WaitingProducerBlockComputed {
        // TODO +_+_+: change this to block digest when off-chain transactions added
        block_hash: H256,
        parent_hash: Option<H256>,
    },
}

impl Verifier {
    pub fn new(
        block: SimpleBlockData,
        producer: Address,
        received_producer_blocks: Vec<SignedData<ProducerBlock>>,
    ) -> Result<(Self, Vec<ControlEvent>), ControlError> {
        let parent_hash = block.header.parent_hash;

        if let Some(producer_block) = received_producer_blocks.into_iter().find(|signed| {
            signed
                .verify_address(producer)
                .map(|_| true)
                .unwrap_or(false)
        }) {
            let mut verifier = Verifier {
                producer,
                block,
                state: VerifierState::WaitingForProducerBlock,
                earlier_validation_request: None,
            };
            verifier
                .receive_block_from_producer(producer_block)
                .map(|events| (verifier, events))
        } else {
            Ok((
                Verifier {
                    producer,
                    block,
                    state: VerifierState::WaitingParentComputed { parent_hash },
                    earlier_validation_request: None,
                },
                vec![ControlEvent::ComputeBlock(parent_hash)],
            ))
        }
    }

    pub fn receive_block_from_producer(
        &mut self,
        signed: SignedData<ProducerBlock>,
    ) -> Result<Vec<ControlEvent>, ControlError> {
        // Verify sender is current block producer
        signed
            .verify_address(self.producer)
            .map_err(|e| {
                ControlError::Warning(anyhow!("Received block is not signed by the producer: {e}"))
            })?;

        let parent_hash_in_computation = match &self.state {
            VerifierState::WaitingParentComputed { parent_hash } => Some(*parent_hash),
            VerifierState::WaitingForProducerBlock => None,
            _ => {
                return Err(ControlError::Warning(anyhow!(
                    "Received not waited producer block"
                )))
            }
        };

        self.state = VerifierState::WaitingProducerBlockComputed {
            block_hash: signed.data().block_hash,
            parent_hash: parent_hash_in_computation,
        };

        let (block, _) = signed.into_parts();
        Ok(vec![ControlEvent::ComputeProducerBlock(block)])
    }

    /// Returns whether the received block is a computed block from the producer
    pub fn receive_computed_block(&mut self, computed_block: H256) -> Result<bool, ControlError> {
        match &mut self.state {
            VerifierState::WaitingProducerBlockComputed {
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
            VerifierState::WaitingParentComputed { parent_hash } => {
                if computed_block == *parent_hash {
                    self.state = VerifierState::WaitingForProducerBlock;
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

        // TODO +_+_+: check also that request is for the current block

        if self.earlier_validation_request.is_some() {
            return Err(ControlError::Warning(anyhow!(
                "Received second validation request"
            )));
        }

        self.earlier_validation_request = Some(request.into_parts().0);

        Ok(())
    }

    pub fn into_parts(self) -> (Address, SimpleBlockData, Option<BatchCommitmentValidationRequest>) {
        (self.producer, self.block, self.earlier_validation_request)
    }
}
