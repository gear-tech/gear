use anyhow::anyhow;
use ethexe_common::SimpleBlockData;
use ethexe_signer::{Address, ToDigest};
use gprimitives::H256;

use crate::bp::{ControlError, ControlEvent, SignedProducerBlock};

pub struct Verifier {
    validators: Vec<Address>,
    producer: Address,
    block: SimpleBlockData,
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
    WaitingForValidationRequest,
}

impl Verifier {
    pub fn new(
        block: SimpleBlockData,
        validators: Vec<Address>,
        producer: Address,
        received_producer_blocks: Vec<SignedProducerBlock>,
    ) -> Result<(Self, Vec<ControlEvent>), ControlError> {
        let parent_hash = block.header.parent_hash;

        if let Some(producer_block) = received_producer_blocks.into_iter().find(|block| {
            block
                .ecdsa_signature
                .recover_from_digest(block.block.to_digest())
                .map(|pk| pk.to_address() == producer)
                .unwrap_or(false)
        }) {
            let mut verifier = Verifier {
                validators,
                producer,
                block,
                state: VerifierState::WaitingForProducerBlock,
            };
            verifier
                .receive_block_from_producer(producer_block)
                .map(|events| (verifier, events))
        } else {
            Ok((
                Verifier {
                    validators,
                    producer,
                    block,
                    state: VerifierState::WaitingParentComputed { parent_hash },
                },
                vec![ControlEvent::ComputeBlock(parent_hash)],
            ))
        }
    }

    pub fn receive_block_from_producer(
        &mut self,
        signed: SignedProducerBlock,
    ) -> Result<Vec<ControlEvent>, ControlError> {
        // Verify sender is current block producer
        signed
            .ecdsa_signature
            .recover_from_digest(signed.block.to_digest())?
            .to_address()
            .eq(&self.producer)
            .then_some(())
            .ok_or_else(|| ControlError::Warning(anyhow!("Received block from wrong producer")))?;

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
            block_hash: signed.block.block_hash,
            parent_hash: parent_hash_in_computation,
        };
        Ok(vec![ControlEvent::ComputeProducerBlock(signed.block)])
    }

    pub fn receive_computed_block(&mut self, computed_block: H256) -> Result<(), ControlError> {
        match &mut self.state {
            VerifierState::WaitingProducerBlockComputed {
                block_hash,
                parent_hash,
            } => {
                if computed_block == *block_hash {
                    self.state = VerifierState::WaitingForValidationRequest;
                    Ok(())
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
                    Ok(())
                } else {
                    Err(ControlError::Warning(anyhow!(
                        "Received computed block {} is different from the expected parent hash {}",
                        computed_block,
                        parent_hash
                    )))
                }
            },
            _ => Err(ControlError::Warning(anyhow!(
                "Received computed block in unexpected state"
            ))),
        }
    }
}
