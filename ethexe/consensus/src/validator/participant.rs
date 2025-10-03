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

use super::{
    DefaultProcessing, PendingEvent, StateHandler, ValidatorContext, ValidatorState,
    initial::Initial,
};
use crate::{
    BatchCommitmentValidationReply, BatchCommitmentValidationRequest, ConsensusEvent,
    SignedValidationRequest,
};
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{Address, Digest, SimpleBlockData};
use futures::{FutureExt, future::BoxFuture};
use std::task::Poll;

/// [`Participant`] is a state of the validator that processes validation requests,
/// which are sent by the current block producer (from the coordinator state).
/// After replying to the request, it switches back to the [`Initial`] state
/// and waits for the next block.
#[derive(Debug, Display)]
#[display("PARTICIPANT in state {state:?}")]
pub struct Participant {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    producer: Address,
    state: State,
}

#[derive(Debug)]
enum State {
    WaitingForValidationRequest,
    ProcessingValidationRequest {
        #[debug(skip)]
        future: BoxFuture<'static, Result<Digest>>,
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
        request: SignedValidationRequest,
    ) -> Result<ValidatorState> {
        if request.address() == self.producer {
            self.process_validation_request(request.into_parts().0)
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
                Ok(digest) => {
                    let reply = self
                        .ctx
                        .core
                        .signer
                        .sign_for_contract(
                            self.ctx.core.router_address,
                            self.ctx.core.pub_key,
                            digest,
                        )
                        .map(|signature| BatchCommitmentValidationReply { digest, signature })?;

                    self.output(ConsensusEvent::PublishValidationReply(reply));
                }
                Err(err) => self.warning(format!("reject validation request: {err}")),
            }

            // NOTE: In both cases it returns to the initial state,
            // means - even if producer publish incorrect validation request,
            // then participant does not wait for the next validation request from producer.
            Initial::create(self.ctx).map(|s| (Poll::Ready(()), s))
        } else {
            Ok((Poll::Pending, self.into()))
        }
    }
}

impl Participant {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        producer: Address,
    ) -> Result<ValidatorState> {
        let mut earlier_validation_request = None;
        ctx.pending_events.retain(|event| match event {
            PendingEvent::ValidationRequest(signed_data)
                if earlier_validation_request.is_none() && signed_data.address() == producer =>
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
            producer,
            state: State::WaitingForValidationRequest,
        };

        let Some(validation_request) = earlier_validation_request else {
            return Ok(participant.into());
        };

        participant.process_validation_request(validation_request)
    }

    fn process_validation_request(
        mut self,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidatorState> {
        let State::WaitingForValidationRequest = self.state else {
            self.warning("unexpected validation request".to_string());
            return Ok(self.into());
        };

        tracing::info!("Start processing validation request");
        self.state = State::ProcessingValidationRequest {
            future: self
                .ctx
                .core
                .clone()
                .validate_batch_commitment_request(self.block.clone(), request)
                .boxed(),
        };

        Ok(self.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        utils::{SignedProducerBlock, SignedValidationRequest},
        validator::mock::*,
    };
    use ethexe_common::{Digest, ToDigest, gear::CodeCommitment};
    use gprimitives::H256;

    #[test]
    fn create() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random());

        let participant = Participant::create(ctx, block, producer.to_address()).unwrap();

        assert!(participant.is_participant());
        assert_eq!(participant.context().pending_events.len(), 0);
    }

    #[tokio::test]
    async fn create_with_pending_events() {
        let (mut ctx, keys, _) = mock_validator_context();
        let producer = keys[0];
        let alice = keys[1];
        let block = SimpleBlockData::mock(H256::random());

        // Validation request from alice - must be kept
        ctx.pending(SignedValidationRequest::mock((
            ctx.core.signer.clone(),
            alice,
            (),
        )));

        // Reply from producer - must be removed and processed
        ctx.pending(SignedValidationRequest::mock((
            ctx.core.signer.clone(),
            producer,
            (),
        )));

        // Block from producer - must be kept
        ctx.pending(SignedProducerBlock::mock((
            ctx.core.signer.clone(),
            producer,
            H256::random(),
        )));

        // Block from alice - must be kept
        ctx.pending(SignedProducerBlock::mock((
            ctx.core.signer.clone(),
            alice,
            H256::random(),
        )));

        let (state, event) = Participant::create(ctx, block, producer.to_address())
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());

        // Pending validation request from producer was found and rejected
        assert!(event.is_warning());

        let ctx = state.into_context();
        assert_eq!(ctx.pending_events.len(), 3);
        assert!(ctx.pending_events[0].is_producer_block());
        assert!(ctx.pending_events[1].is_producer_block());
        assert!(ctx.pending_events[2].is_validation_request());
    }

    #[tokio::test]
    async fn process_validation_request_success() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.core.db);
        let block = simple_block_data(&ctx.core.db, batch.block_hash);

        let signed_request = ctx
            .core
            .signer
            .signed_data(producer, BatchCommitmentValidationRequest::new(&batch))
            .unwrap();

        let state = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());

        let ConsensusEvent::PublishValidationReply(reply) = event else {
            panic!("Expected PublishValidationReply event, got {event:?}");
        };
        assert_eq!(reply.digest, batch.to_digest());
        reply
            .signature
            .validate(state.context().core.router_address, reply.digest)
            .unwrap();
    }

    #[tokio::test]
    async fn process_validation_request_failure() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random());
        let signed_request = SignedValidationRequest::mock((ctx.core.signer.clone(), producer, ()));

        let state = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(matches!(event, ConsensusEvent::Warning(_)));
    }

    #[tokio::test]
    async fn codes_not_waiting_for_commitment_error() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let mut batch = prepared_mock_batch_commitment(&ctx.core.db);
        let block = simple_block_data(&ctx.core.db, batch.block_hash);

        // Add a code that's not in the waiting queue
        let extra_code = CodeCommitment::mock(());
        batch.code_commitments.push(extra_code);

        let request = BatchCommitmentValidationRequest::new(&batch);
        let signed_request = ctx.core.signer.signed_data(producer, request).unwrap();

        let state = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_warning());
    }

    #[tokio::test]
    async fn empty_batch_error() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.core.db, H256::random());

        // Create a request with empty blocks and codes
        let request = BatchCommitmentValidationRequest {
            digest: Digest::random(),
            head: None,
            codes: vec![],
            rewards: false,
            validators: false,
        };

        let signed_request = ctx.core.signer.signed_data(producer, request).unwrap();

        let state = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_warning());
    }

    #[tokio::test]
    async fn duplicate_codes_warning() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.core.db);
        let block = simple_block_data(&ctx.core.db, batch.block_hash);

        // Create a request with duplicate codes
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        if !request.codes.is_empty() {
            let duplicate_code = request.codes[0];
            request.codes.push(duplicate_code);
        }

        let signed_request = ctx.core.signer.signed_data(producer, request).unwrap();

        let state = Participant::create(ctx, block.clone(), producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_warning());
    }

    #[tokio::test]
    async fn digest_mismatch_warning() {
        let (ctx, pub_keys, _) = mock_validator_context();
        let producer = pub_keys[0];
        let batch = prepared_mock_batch_commitment(&ctx.core.db);
        let block = simple_block_data(&ctx.core.db, batch.block_hash);

        // Create request with incorrect digest
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        request.digest = Digest::random();

        let signed_request = ctx.core.signer.signed_data(producer, request).unwrap();

        let state = Participant::create(ctx, block, producer.to_address()).unwrap();
        assert!(state.is_participant());

        let (state, event) = state
            .process_validation_request(signed_request)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_warning());
    }
}
