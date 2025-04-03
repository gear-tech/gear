use anyhow::{anyhow, ensure, Result};
use ethexe_common::gear::BatchCommitment;
use ethexe_ethereum::router::Router;
use ethexe_signer::Address;
use futures::{future::BoxFuture, FutureExt};
use gprimitives::H256;
use std::{
    collections::BTreeSet,
    task::{Context, Poll},
};

use super::{InputEvent, ValidatorContext, ValidatorSubService};
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

    fn process_input_event(
        mut self: Box<Self>,
        event: InputEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        match event {
            InputEvent::ValidationReply(reply) => {
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
                    self.ctx.output.push_back(ControlEvent::Warning(format!(
                        "COORDINATOR - validation reply rejected: {err}"
                    )))
                }

                if self.multisigned_batch.signatures().len() as u64 >= self.ctx.threshold {
                    Submitter::create(self.ctx, self.multisigned_batch)
                } else {
                    Ok(self)
                }
            }
            event => {
                self.ctx.warning(format!(
                    "COORDINATOR - received unexpected event: {event:?}"
                ));

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

        ctx.output
            .push_back(ControlEvent::PublishValidationRequest(validation_request));

        Ok(Box::new(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }))
    }
}

struct Submitter {
    ctx: ValidatorContext,
    future: BoxFuture<'static, Result<H256>>,
}

impl ValidatorSubService for Submitter {
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

    fn process_input_event(
        mut self: Box<Self>,
        event: InputEvent,
    ) -> Result<Box<dyn ValidatorSubService>> {
        self.ctx.pending_events.push_back(event);
        Ok(self)
    }

    fn poll(mut self: Box<Self>, cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(Ok(tx)) => self
                .ctx
                .output
                .push_back(ControlEvent::CommitmentSubmitted(tx)),
            Poll::Ready(Err(err)) => self.ctx.output.push_back(ControlEvent::Warning(format!(
                "Failed to submit batch commitment: {err:?}"
            ))),
            Poll::Pending => {}
        }
        Ok(self)
    }
}

impl Submitter {
    pub fn create(
        ctx: ValidatorContext,
        batch: MultisignedBatchCommitment,
    ) -> Result<Box<dyn ValidatorSubService>> {
        let router = ctx.ethereum.router();

        Ok(Box::new(Self {
            ctx,
            future: submit_batch_commitment(router, batch).boxed(),
        }))
    }
}

async fn submit_batch_commitment(
    router: Router,
    batch: MultisignedBatchCommitment,
) -> Result<H256> {
    let (commitment, signatures) = batch.into_parts();
    let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

    log::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

    router.commit_batch(commitment, signatures).await
}
