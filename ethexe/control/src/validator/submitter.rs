use anyhow::Result;
use async_trait::async_trait;
use ethexe_ethereum::router::Router;
use futures::{future::BoxFuture, FutureExt};
use gprimitives::H256;
use std::task::{Context, Poll};

use super::{initial::Initial, BatchCommitter, ValidatorContext, ValidatorSubService};
use crate::{utils::MultisignedBatchCommitment, ControlEvent};

pub struct Submitter {
    ctx: ValidatorContext,
    future: BoxFuture<'static, Result<H256>>,
}

impl ValidatorSubService for Submitter {
    fn log(&self, s: String) -> String {
        format!("SUBMITTER - {s}")
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
                self.output(ControlEvent::CommitmentSubmitted(tx));

                Initial::create(self.ctx)
            }
            Poll::Ready(Err(err)) => {
                self.warning(format!("failed to submit batch commitment: {err:?}"));

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
        let future = ctx.committer.clone_boxed().commit_batch(batch);
        Ok(Box::new(Self { ctx, future }))
    }
}

#[derive(Clone)]
pub struct EthereumCommitter {
    pub(crate) router: Router,
}

#[async_trait]
impl BatchCommitter for EthereumCommitter {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(self.clone())
    }

    async fn commit_batch(self: Box<Self>, batch: MultisignedBatchCommitment) -> Result<H256> {
        let (commitment, signatures) = batch.into_parts();
        let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

        log::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

        self.router.commit_batch(commitment, signatures).await
    }
}

// async fn submit_batch_commitment(
//     router: Router,
//     batch: MultisignedBatchCommitment,
// ) -> Result<H256> {
//     let (commitment, signatures) = batch.into_parts();
//     let (origins, signatures): (Vec<_>, _) = signatures.into_iter().unzip();

//     log::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

//     router.commit_batch(commitment, signatures).await
// }
