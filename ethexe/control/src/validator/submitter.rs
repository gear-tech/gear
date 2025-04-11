use anyhow::Result;
use async_trait::async_trait;
use derivative::Derivative;
use ethexe_ethereum::router::Router;
use futures::{future::BoxFuture, FutureExt};
use gprimitives::H256;
use std::{
    fmt,
    task::{Context, Poll},
};

use super::{initial::Initial, BatchCommitter, ValidatorContext, ValidatorSubService};
use crate::{utils::MultisignedBatchCommitment, ControlEvent};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Submitter {
    ctx: ValidatorContext,
    #[derivative(Debug = "ignore")]
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

impl fmt::Display for Submitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SUBMITTER")
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

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::*;
    use crate::{tests::*, validator::tests::*};
    use ethexe_common::gear::BatchCommitment;

    #[tokio::test]
    async fn submitter() {
        let (ctx, _) = mock_validator_context();
        let batch = BatchCommitment {
            code_commitments: vec![mock_code_commitment(), mock_code_commitment()],
            block_commitments: vec![
                mock_block_commitment(H256::random(), H256::random(), H256::random()).1,
            ],
        };

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &ctx.signer.contract_signer(ctx.router_address),
            ctx.pub_key,
        )
        .unwrap();

        let submitter = Submitter::create(ctx, multisigned_batch.clone()).unwrap();
        assert_eq!(submitter.type_id(), TypeId::of::<Submitter>());

        let (initial, event) = submitter.wait_for_event().await.unwrap();
        assert_eq!(initial.type_id(), TypeId::of::<Initial>());
        assert!(matches!(event, ControlEvent::CommitmentSubmitted(_)));

        with_batch(|submitted_batch| assert_eq!(submitted_batch, Some(&multisigned_batch)));
    }
}
