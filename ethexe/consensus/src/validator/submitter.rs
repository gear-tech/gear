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

use super::{initial::Initial, BatchCommitter, StateHandler, ValidatorContext};
use crate::{utils::MultisignedBatchCommitment, ConsensusEvent};
use anyhow::Result;
use async_trait::async_trait;
use derive_more::{Debug, Display};
use ethexe_ethereum::router::Router;
use futures::{future::BoxFuture, FutureExt};
use gprimitives::H256;
use std::task::{Context, Poll};

#[derive(Debug, Display)]
#[display("SUBMITTER")]
pub struct Submitter {
    ctx: ValidatorContext,
    #[debug(skip)]
    future: BoxFuture<'static, Result<H256>>,
}

impl StateHandler for Submitter {
    fn into_dyn(self: Box<Self>) -> Box<dyn StateHandler> {
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

    fn poll(mut self: Box<Self>, cx: &mut Context<'_>) -> Result<Box<dyn StateHandler>> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(Ok(tx)) => {
                self.output(ConsensusEvent::CommitmentSubmitted(tx));

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
    ) -> Result<Box<dyn StateHandler>> {
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
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::gear::BatchCommitment;
    use std::any::TypeId;

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
        assert!(matches!(event, ConsensusEvent::CommitmentSubmitted(_)));

        with_batch(|submitted_batch| assert_eq!(submitted_batch, Some(&multisigned_batch)));
    }
}
