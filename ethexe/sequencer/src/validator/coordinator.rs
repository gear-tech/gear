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
    state: State,
}

enum State {
    WaitingForValidationReplies,
    Final,
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

        if threshold == 1 {
            Ok((
                Self {
                    multisigned_batch,
                    validators: validators.into_iter().collect(),
                    threshold,
                    state: State::Final,
                },
                vec![],
            ))
        } else {
            Ok((
                Self {
                    multisigned_batch,
                    validators: validators.into_iter().collect(),
                    threshold,
                    state: State::WaitingForValidationReplies,
                },
                vec![ControlEvent::PublishValidationRequest(
                    signed_validation_request,
                )],
            ))
        }
    }

    pub fn receive_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<(), ControlError> {
        // NOTE: receiving in the final state also allowed

        self.multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Received validation reply is not from validator"))
            })
            .map_err(|e| ControlError::Warning(anyhow!("Validation rejected: {e}")))?;

        if self.multisigned_batch.signatures().len() as u64 >= self.threshold {
            self.state = State::Final;
        }

        Ok(())
    }

    pub fn is_final(&self) -> bool {
        matches!(self.state, State::Final)
    }

    pub fn into_multisigned_batch_commitment(self) -> MultisignedBatchCommitment {
        if !self.is_final() {
            unreachable!("Coordinator is not in the final state: wrong Coordinator usage");
        }

        self.multisigned_batch
    }
}
