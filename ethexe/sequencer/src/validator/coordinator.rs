use crate::{
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        MultisignedBatchCommitment,
    },
    ControlError, ControlEvent,
};
use anyhow::anyhow;
use ethexe_common::gear::BatchCommitment;
use ethexe_signer::{Address, PublicKey, Signer};
use std::collections::BTreeSet;

pub struct Coordinator {
    multisigned_batch: MultisignedBatchCommitment,
    validators: BTreeSet<Address>,
    threshold: u64,
}

impl Coordinator {
    pub fn new(
        pub_key: PublicKey,
        validators: Vec<Address>,
        threshold: u64,
        router_address: Address,
        batch: BatchCommitment,
        signer: Signer,
    ) -> Result<(Self, Vec<ControlEvent>), anyhow::Error> {
        let validation_request = BatchCommitmentValidationRequest::from(&batch);
        let signed_validation_request = signer.create_signed_data(pub_key, validation_request)?;
        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &signer.contract_signer(router_address),
            pub_key,
        )?;

        Ok((
            Self {
                multisigned_batch,
                validators: validators.into_iter().collect(),
                threshold,
            },
            vec![ControlEvent::PublishValidationRequest(
                signed_validation_request,
            )],
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
                    .ok_or_else(|| anyhow!("Received validation reply is not from validator"))
            })
            .map_err(|e| ControlError::Warning(anyhow!("Validation rejected: {e}")))?;

        Ok(self.multisigned_batch.signatures().len() as u64 >= self.threshold)
    }

    pub fn into_multisigned_batch_commitment(self) -> MultisignedBatchCommitment {
        self.multisigned_batch
    }
}
