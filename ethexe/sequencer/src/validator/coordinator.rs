use anyhow::{anyhow, ensure, Result};
use ethexe_common::gear::BatchCommitment;
use ethexe_signer::Address;
use std::collections::BTreeSet;

use super::{submitter::Submitter, ExternalEvent, ValidatorContext, ValidatorSubService};
use crate::{
    utils::{BatchCommitmentValidationRequest, MultisignedBatchCommitment},
    ControlEvent,
};

pub struct Coordinator {
    ctx: ValidatorContext,
    validators: BTreeSet<Address>,
    multisigned_batch: MultisignedBatchCommitment,
}

impl ValidatorSubService for Coordinator {
    fn log(&self, s: String) -> String {
        format!("COORDINATOR - {s}")
    }

    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_external_event(
        mut self: Box<Self>,
        event: ExternalEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match event {
            ExternalEvent::ValidationReply(reply) => {
                if let Err(err) = self
                    .multisigned_batch
                    .accept_batch_commitment_validation_reply(reply, |addr| {
                        self.validators
                            .contains(&addr)
                            .then_some(())
                            .ok_or_else(|| {
                                anyhow!("Received validation reply is not from known validator")
                            })
                    })
                {
                    self.warning(format!("validation reply rejected: {err}"));
                }

                if self.multisigned_batch.signatures().len() as u64 >= self.ctx.threshold {
                    Submitter::create(self.ctx, self.multisigned_batch)
                } else {
                    Ok(self)
                }
            }
            event => {
                self.warning(format!("unexpected event: {event:?}, saved for later"));

                self.ctx.pending_events.push_back(event);

                Ok(self)
            }
        }
    }
}

impl Coordinator {
    pub fn create(
        mut ctx: ValidatorContext,
        validators: Vec<Address>,
        batch: BatchCommitment,
    ) -> Result<Box<dyn ValidatorSubService>> {
        ensure!(
            validators.len() as u64 >= ctx.threshold,
            "Number of validators is less than threshold"
        );

        ensure!(ctx.threshold > 0, "Threshold should be greater than 0");

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &ctx.signer.contract_signer(ctx.router_address),
            ctx.pub_key,
        )?;

        if multisigned_batch.signatures().len() as u64 >= ctx.threshold {
            return Submitter::create(ctx, multisigned_batch);
        }

        let validation_request = ctx.signer.create_signed_data(
            ctx.pub_key,
            BatchCommitmentValidationRequest::from(multisigned_batch.batch()),
        )?;

        ctx.output(ControlEvent::PublishValidationRequest(validation_request));

        Ok(Box::new(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }))
    }
}
