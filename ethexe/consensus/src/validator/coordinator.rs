// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`Coordinator`] aggregates finalized MBs into a [`BatchCommitment`],
//! gossips a validation request, collects threshold-many signatures, and
//! submits the multi-signed batch to the Router.
//!
//! The coordinator is elected per Ethereum block via
//! [`ProtocolTimelines::block_coordinator_at`]. A new chain head always
//! aborts the current attempt.

use super::{StateHandler, ValidatorContext, ValidatorState, idle::Idle};
use crate::{
    BatchCommitmentValidationReply, CommitmentSubmitted, ConsensusEvent,
    utils::MultisignedBatchCommitment,
};
use anyhow::{Context as _, Result, anyhow, ensure};
use derive_more::Display;
use ethexe_common::{
    Address, SimpleBlockData, ToDigest, ValidatorsVec, consensus::BatchCommitmentValidationRequest,
    gear::BatchCommitment, network::ValidatorMessage,
};
use futures::{FutureExt, future::BoxFuture};
use gsigner::secp256k1::Secp256k1SignerExt;
use std::{
    collections::BTreeSet,
    task::{Context, Poll},
};
use tokio::time::sleep;

/// Pre-coordinator state that holds off batch aggregation for
/// [`ValidatorCore::coordinator_aggregation_delay`]. The delay buys
/// participants time to receive the same chain head and lets compute
/// finish executing whatever MB it picked up from the proposal.
///
/// After the delay elapses, [`CoordinatorBoot`] aggregates the batch and
/// either transitions to [`Coordinator`] (gossiping a validation request)
/// or returns to [`Idle`] (nothing to commit).
#[derive(Display)]
#[display("COORDINATOR_BOOT")]
pub struct CoordinatorBoot {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: ValidatorsVec,
    /// `Some` while we're either sleeping or awaiting the batch builder.
    pending: Option<BoxFuture<'static, Result<Option<BatchCommitment>>>>,
}

impl std::fmt::Debug for CoordinatorBoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoordinatorBoot")
            .field("block", &self.block.hash)
            .finish_non_exhaustive()
    }
}

impl CoordinatorBoot {
    pub fn start(
        ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: ValidatorsVec,
    ) -> Result<ValidatorState> {
        let delay = ctx.core.coordinator_aggregation_delay;
        let batch_manager = ctx.core.batch_manager.clone();

        // Schedule the delayed aggregation as a single boxed future. The
        // state machine drives it via `poll_next_state`.
        let pending = async move {
            sleep(delay).await;
            batch_manager.create_batch_commitment(block).await
        }
        .boxed();

        Ok(Self {
            ctx,
            block,
            validators,
            pending: Some(pending),
        }
        .into())
    }
}

impl StateHandler for CoordinatorBoot {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        let Some(future) = self.pending.as_mut() else {
            return Ok((Poll::Pending, self.into()));
        };

        match future.poll_unpin(cx) {
            Poll::Pending => Ok((Poll::Pending, self.into())),
            Poll::Ready(Err(err)) => Err(err),
            Poll::Ready(Ok(None)) => {
                // Empty batch — coordinator has nothing to commit. Drop back
                // to Idle and wait for the next chain head.
                tracing::debug!(
                    block = %self.block.hash,
                    "coordinator skipped batch: no commitments to submit"
                );
                let next = Idle::create(self.ctx)?;
                Ok((Poll::Ready(()), next))
            }
            Poll::Ready(Ok(Some(batch))) => {
                let next = Coordinator::create(self.ctx, self.validators, batch, self.block)?;
                Ok((Poll::Ready(()), next))
            }
        }
    }
}

/// [`Coordinator`] sends a batch commitment validation request to other
/// validators and waits for replies. Switches to a submission task once
/// it has accumulated the threshold-many signatures.
#[derive(Debug, Display)]
#[display("COORDINATOR")]
pub struct Coordinator {
    ctx: ValidatorContext,
    validators: BTreeSet<Address>,
    multisigned_batch: MultisignedBatchCommitment,
}

impl StateHandler for Coordinator {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_reply(
        mut self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        if let Err(err) = self
            .multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Received validation reply is not known validator"))
            })
        {
            self.warning(format!("validation reply rejected: {err}"));
        }

        if self.multisigned_batch.signatures().len() as u64 >= self.ctx.core.signatures_threshold {
            Self::submission(self.ctx, self.multisigned_batch)
        } else {
            Ok(self.into())
        }
    }
}

impl Coordinator {
    pub fn create(
        mut ctx: ValidatorContext,
        validators: ValidatorsVec,
        batch: BatchCommitment,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        debug_assert_eq!(batch.block_hash, block.hash, "Block hash mismatch");
        ensure!(
            validators.len() as u64 >= ctx.core.signatures_threshold,
            "Number of validators is less than threshold"
        );

        ensure!(
            ctx.core.signatures_threshold > 0,
            "Threshold should be greater than 0"
        );

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &ctx.core.signer,
            ctx.core.router_address,
            ctx.core.pub_key,
        )?;

        ctx.core
            .metrics
            .last_signed_commitment_block_number
            .set(block.header.height);

        if multisigned_batch.signatures().len() as u64 >= ctx.core.signatures_threshold {
            return Self::submission(ctx, multisigned_batch);
        }

        let era_index = ctx
            .core
            .timelines
            .era_from_ts(multisigned_batch.batch().timestamp)
            .context("failed to calculate era from batch timestamp")?;
        let payload = BatchCommitmentValidationRequest::new(multisigned_batch.batch());
        let message = ValidatorMessage { era_index, payload };

        let validation_request = ctx
            .core
            .signer
            .signed_data(ctx.core.pub_key, message, None)?;

        ctx.output(ConsensusEvent::PublishMessage(validation_request.into()));

        Ok(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }
        .into())
    }

    pub fn submission(
        ctx: ValidatorContext,
        multisigned_batch: MultisignedBatchCommitment,
    ) -> Result<ValidatorState> {
        let (batch, signatures) = multisigned_batch.into_parts();
        let cloned_committer = ctx.core.committer.clone_boxed();
        ctx.tasks.push(
            async move {
                let block_hash = batch.block_hash;
                let batch_digest = batch.to_digest();
                let event = match cloned_committer.commit(batch, signatures).await {
                    Ok(tx) => CommitmentSubmitted {
                        block_hash,
                        batch_digest,
                        tx,
                    }.into(),
                    Err(err) => ConsensusEvent::Warning(format!(
                        "Failed to submit commitment for block {block_hash}, digest {batch_digest}: {err}"
                    ))
                };
                Ok(event)
            }
            .boxed(),
        );
        Idle::create(ctx)
    }
}
