use std::task::{Context, Poll};

use anyhow::Result;
use ethexe_ethereum::router::Router;
use futures::{future::BoxFuture, FutureExt};
use gprimitives::H256;

use crate::{utils::MultisignedBatchCommitment, ControlEvent};

use super::{initial::Initial, ValidatorContext, ValidatorSubService};

pub struct Submitter {
    ctx: ValidatorContext,
    future: BoxFuture<'static, Result<H256>>,
}

impl ValidatorSubService for Submitter {
    fn log(&self, s: String) -> String {
        format!("Submitter - {s}")
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

    fn poll(mut self: Box<Self>, cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(Ok(tx)) => {
                self.ctx
                    .output
                    .push_back(ControlEvent::CommitmentSubmitted(tx));
                Initial::create(self.ctx)
            }
            Poll::Ready(Err(err)) => {
                let warning = self.log(format!("failed to submit batch commitment: {err:?}"));
                self.ctx.warning(warning);
                Initial::create(self.ctx)
            }
            Poll::Pending => Ok(self),
        }
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
