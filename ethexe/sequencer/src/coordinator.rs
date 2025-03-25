use crate::{
    bp::{ControlError, ControlEvent},
    producer::Producer,
    utils::{BatchCommitmentValidationReply, MultisignedBatchCommitment},
};
use anyhow::anyhow;
use ethexe_common::gear::BatchCommitment;
use ethexe_signer::Address;
use std::collections::BTreeSet;

pub struct Coordinator {
    multisigned_batch: MultisignedBatchCommitment,
    validators: BTreeSet<Address>,
    threshold: u64,
}

impl Coordinator {
    pub fn new(
        batch: BatchCommitment,
        producer: Producer,
        threshold: u64,
    ) -> Result<(Self, Vec<ControlEvent>), anyhow::Error> {
        let (pub_key, signer, _, validators, _) = producer.into_parts();

        let (multisigned_batch, validation_request) =
            MultisignedBatchCommitment::new_with_validation_request(batch, &signer, pub_key)?;

        Ok((
            Self {
                multisigned_batch,
                validators: validators.into_iter().collect(),
                threshold,
            },
            vec![ControlEvent::PublishValidationRequest(validation_request)],
        ))
    }

    pub fn receive_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<bool, ControlError> {
        self.multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Receive validation reply not from validator"))
            })
            .map_err(|e| ControlError::Warning(anyhow!("Validation rejected: {e}")))?;

        Ok(self.multisigned_batch.signatures().len() as u64 >= self.threshold)
    }

    pub fn into_multisigned_batch_commitment(self) -> MultisignedBatchCommitment {
        self.multisigned_batch
    }
}
