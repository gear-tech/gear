// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::{BatchCommitter, StateHandler, ValidatorContext, ValidatorState, initial::Initial};
use crate::{ConsensusEvent, utils::MultisignedBatchCommitment};
use anyhow::Result;
use async_trait::async_trait;
use derive_more::{Debug, Display};
use ethexe_ethereum::router::Router;
use futures::{FutureExt, future::BoxFuture};
use gprimitives::H256;
use std::task::{Context, Poll};

/// [`Submitter`] is the last state of the current block producer validator.
/// It submits the batch commitment to the Ethereum network.
/// After the submission it switches to [`Initial`] state.
#[derive(Debug, Display)]
#[display("SUBMITTER")]
pub struct Submitter {
    ctx: ValidatorContext,
    #[debug(skip)]
    future: BoxFuture<'static, Result<H256>>,
}

impl StateHandler for Submitter {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(Ok(tx)) => {
                self.output(ConsensusEvent::CommitmentSubmitted(tx));

                Initial::create(self.ctx)
            }
            Poll::Ready(Err(err)) => {
                // TODO: consider retries
                self.warning(format!("failed to submit batch commitment: {err:?}"));

                Initial::create(self.ctx)
            }
            Poll::Pending => Ok(self.into()),
        }
    }
}

impl Submitter {
    pub fn create(
        ctx: ValidatorContext,
        batch: MultisignedBatchCommitment,
    ) -> Result<ValidatorState> {
        let future = ctx.committer.clone_boxed().commit_batch(batch);
        Ok(Self { ctx, future }.into())
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

        tracing::debug!("Batch commitment to submit: {commitment:?}, signed by: {origins:?}");

        self.router.commit_batch(commitment, signatures).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::gear::BatchCommitment;

    #[tokio::test]
    async fn submitter() {
        let (ctx, _) = mock_validator_context();
        let batch = BatchCommitment::mock(());
        let multisigned_batch =
            MultisignedBatchCommitment::new(batch, &ctx.signer, ctx.router_address, ctx.pub_key)
                .unwrap();

        let submitter = Submitter::create(ctx, multisigned_batch.clone()).unwrap();
        assert!(submitter.is_submitter());

        let (initial, event) = submitter.wait_for_event().await.unwrap();
        assert!(initial.is_initial());
        assert!(matches!(event, ConsensusEvent::CommitmentSubmitted(_)));

        with_batch(|submitted_batch| assert_eq!(submitted_batch, Some(&multisigned_batch)));
    }
}
