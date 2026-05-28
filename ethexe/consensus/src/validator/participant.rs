// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`Participant`] receives a validation request from the coordinator,
//! re-derives the batch independently, and replies with a signature on the
//! resulting digest. After replying it returns to [`Idle`].

use super::{
    DefaultProcessing, PendingEvent, StateHandler, ValidatorContext, ValidatorState, idle::Idle,
};
use crate::{BatchCommitmentValidationReply, ConsensusEvent, validator::batch::ValidationStatus};

use anyhow::{Context as _, Result};
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, SimpleBlockData,
    consensus::{BatchCommitmentValidationRequest, VerifiedValidationRequest},
    network::ValidatorMessage,
};
use futures::{FutureExt, future::BoxFuture};
use gsigner::secp256k1::Secp256k1SignerExt;
use std::task::Poll;

#[derive(Debug, Display)]
#[display("PARTICIPANT in state {state:?}")]
pub struct Participant {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    coordinator: Address,
    state: State,
}

#[derive(Debug)]
enum State {
    WaitingForValidationRequest,
    ProcessingValidationRequest {
        #[debug(skip)]
        future: BoxFuture<'static, Result<ValidationStatus>>,
    },
}

impl StateHandler for Participant {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.coordinator {
            self.process_coordinator_request(request.into_parts().0)
        } else {
            DefaultProcessing::validation_request(self, request)
        }
    }

    fn poll_next_state(
        mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Result<(Poll<()>, ValidatorState)> {
        if let State::ProcessingValidationRequest { future } = &mut self.state
            && let Poll::Ready(res) = future.poll_unpin(cx)
        {
            match res {
                Ok(ValidationStatus::Accepted(digest)) => {
                    let signature = self.ctx.core.signer.sign_for_contract_digest(
                        self.ctx.core.router_address,
                        self.ctx.core.pub_key,
                        digest,
                        None,
                    )?;
                    self.ctx
                        .core
                        .metrics
                        .last_signed_commitment_block_number
                        .set(self.block.header.height);

                    let reply = BatchCommitmentValidationReply { digest, signature };

                    let era_index = self
                        .ctx
                        .core
                        .timelines
                        .era_from_ts(self.block.header.timestamp)
                        .context("failed to calculate era from block timestamp")?;
                    let reply = ValidatorMessage {
                        era_index,
                        payload: reply,
                    };

                    let reply =
                        self.ctx
                            .core
                            .signer
                            .signed_data(self.ctx.core.pub_key, reply, None)?;

                    self.ctx
                        .output(ConsensusEvent::PublishMessage(reply.into()));
                }
                Ok(ValidationStatus::Rejected { request, reason }) => {
                    self.warning(format!("reject validation request {request:?} : {reason}"));
                }
                Err(err) => return Err(err),
            }

            // After replying (or rejecting), return to idle. Even if the
            // coordinator's request was bad we don't wait for a retry —
            // next chain head triggers the next round.
            Idle::create(self.ctx).map(|s| (Poll::Ready(()), s))
        } else {
            Ok((Poll::Pending, self.into()))
        }
    }
}

impl Participant {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        coordinator: Address,
    ) -> Result<ValidatorState> {
        let mut earlier_validation_request = None;
        ctx.pending_events.retain(|event| match event {
            PendingEvent::ValidationRequest(signed_data)
                if earlier_validation_request.is_none() && signed_data.address() == coordinator =>
            {
                earlier_validation_request = Some(signed_data.data().clone());

                false
            }
            _ => {
                // NOTE: keep all other events in queue.
                true
            }
        });

        let participant = Self {
            ctx,
            block,
            coordinator,
            state: State::WaitingForValidationRequest,
        };

        let Some(validation_request) = earlier_validation_request else {
            return Ok(participant.into());
        };

        participant.process_coordinator_request(validation_request)
    }

    fn process_coordinator_request(
        mut self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidatorState> {
        let State::WaitingForValidationRequest = self.state else {
            self.warning("unexpected validation request".to_string());
            return Ok(self.into());
        };

        self.state = State::ProcessingValidationRequest {
            future: self
                .ctx
                .core
                .batch_manager
                .clone()
                .validate_batch_commitment(self.block, request)
                .boxed(),
        };

        Ok(self.into())
    }
}
