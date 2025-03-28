use crate::{
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        MultisignedBatchCommitment,
    },
    ControlEvent,
};
use anyhow::{anyhow, ensure};
use ethexe_common::gear::BatchCommitment;
use ethexe_signer::{Address, PublicKey, Signer};
use std::{
    collections::BTreeSet,
    pin::Pin,
    task::{Context, Poll},
    vec,
};

pub struct Coordinator {
    signer: Signer,
    pub_key: PublicKey,
    multisigned_batch: MultisignedBatchCommitment,
    validators: BTreeSet<Address>,
    threshold: u64,
    state: State,
}

enum State {
    Initial,
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
    ) -> Result<Self, anyhow::Error> {
        ensure!(
            validators.len() as u64 >= threshold,
            "Number of validators is less than threshold"
        );

        ensure!(threshold > 0, "Threshold should be greater than 0");

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &signer.contract_signer(router_address),
            pub_key,
        )?;

        Ok(Self {
            signer,
            pub_key,
            multisigned_batch,
            validators: validators.into_iter().collect(),
            threshold,
            state: State::Initial,
        })
    }

    pub fn receive_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<Vec<ControlEvent>, anyhow::Error> {
        // NOTE: receiving in the final state also allowed

        if let Err(err) = self
            .multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Received validation reply is not from known validator"))
            })
        {
            Ok(vec![ControlEvent::Warning(format!(
                "Validation rejected: {err}"
            ))])
        } else {
            Ok(vec![])
        }
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

impl Future for Coordinator {
    type Output = anyhow::Result<Vec<ControlEvent>>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut self.state {
            State::Initial => {
                let validation_request =
                    BatchCommitmentValidationRequest::from(self.multisigned_batch.batch());
                let res = self
                    .signer
                    .create_signed_data(self.pub_key, validation_request)
                    .map(|signed| {
                        self.state = State::WaitingForValidationReplies;
                        vec![ControlEvent::PublishValidationRequest(signed)]
                    });
                Poll::Ready(res)
            }
            State::WaitingForValidationReplies => {
                if self.multisigned_batch.signatures().len() as u64 >= self.threshold {
                    self.state = State::Final;
                    Poll::Ready(Ok(vec![]))
                } else {
                    Poll::Pending
                }
            }
            State::Final => unreachable!("Coordinator is in the final state"),
        }
    }
}
