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
    StateHandler, ValidatorContext, ValidatorState, coordinator::Coordinator, initial::Initial,
};
use crate::ConsensusEvent;
use anyhow::Result;
use derive_more::{Debug, Display};
use ethexe_common::{Address, ProducerBlock, SimpleBlockData, gear::BatchCommitment};
use ethexe_service_utils::Timer;
use futures::{FutureExt, future::BoxFuture};
use gprimitives::H256;
use nonempty::NonEmpty;
use std::task::{Context, Poll};

/// [`Producer`] is the state of the validator, which creates a new block
/// and publish it to the network. It waits for the block to be computed
/// and then switches to [`Coordinator`] state.
#[derive(Debug, Display)]
#[display("PRODUCER in {:?}", self.state)]
pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: NonEmpty<Address>,
    state: State,
}

#[derive(Debug)]
enum State {
    CollectCodes {
        #[debug(skip)]
        timer: Timer,
    },
    WaitingBlockComputed,
    AggregateBatchCommitment {
        #[debug(skip)]
        future: BoxFuture<'static, Result<Option<BatchCommitment>>>,
    },
}

impl StateHandler for Producer {
    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self) -> ValidatorContext {
        self.ctx
    }

    fn process_computed_block(mut self, computed_block: H256) -> Result<ValidatorState> {
        if !matches!(&self.state, State::WaitingBlockComputed if self.block.hash == computed_block)
        {
            self.warning(format!("unexpected computed block {computed_block}"));

            return Ok(self.into());
        }

        self.state = State::AggregateBatchCommitment {
            future: self
                .ctx
                .core
                .clone()
                .aggregate_batch_commitment(self.block.clone())
                .boxed(),
        };

        Ok(self.into())
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        match &mut self.state {
            State::CollectCodes { timer } => {
                if timer.poll_unpin(cx).is_ready() {
                    self.create_producer_block()?
                }
            }
            State::WaitingBlockComputed => {}
            State::AggregateBatchCommitment { future } => match future.poll_unpin(cx) {
                Poll::Ready(Ok(Some(batch))) => {
                    return Coordinator::create(self.ctx, self.validators, batch)
                        .map(|s| (Poll::Ready(()), s));
                }
                Poll::Ready(Ok(None)) => {
                    log::info!("No commitments - skip batch commitment");
                    return Initial::create(self.ctx).map(|s| (Poll::Ready(()), s));
                }
                Poll::Ready(Err(err)) => {
                    return Err(err);
                }
                Poll::Pending => {}
            },
        }

        Ok((Poll::Pending, self.into()))
    }
}

impl Producer {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: NonEmpty<Address>,
    ) -> Result<ValidatorState> {
        assert!(
            validators.contains(&ctx.core.pub_key.to_address()),
            "Producer is not in the list of validators"
        );

        let mut timer = Timer::new("collect codes", ctx.core.slot_duration / 6);
        timer.start(());

        ctx.pending_events.clear();

        Ok(Self {
            ctx,
            block,
            validators,
            state: State::CollectCodes { timer },
        }
        .into())
    }

    fn create_producer_block(&mut self) -> Result<()> {
        let pb = ProducerBlock {
            block_hash: self.block.hash,
            // TODO #4638: set gas allowance here
            gas_allowance: None,
            // TODO #4639: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_pb = self
            .ctx
            .core
            .signer
            .signed_data(self.ctx.core.pub_key, pb.clone())?;

        self.state = State::WaitingBlockComputed;
        self.output(ConsensusEvent::PublishProducerBlock(signed_pb));
        self.output(ConsensusEvent::ComputeProducerBlock(pb));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SignedValidationRequest, mock::*, validator::mock::*};
    use async_trait::async_trait;
    use ethexe_common::{Digest, ToDigest, db::BlockMetaStorageWrite, gear::CodeCommitment};
    use nonempty::{NonEmpty, nonempty};

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random());

        ctx.pending(SignedValidationRequest::mock((
            ctx.core.signer.clone(),
            keys[0],
            (),
        )));

        let producer = Producer::create(ctx, block, validators.clone()).unwrap();

        let ctx = producer.context();
        assert_eq!(
            ctx.pending_events.len(),
            0,
            "Producer must ignore external events"
        );
    }

    #[tokio::test]
    async fn simple() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.core.db, H256::random());

        let state = Producer::create(ctx, block.clone(), validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap()
            .process_computed_block(block.hash)
            .unwrap()
            .wait_for_initial()
            .await
            .unwrap();

        // No commitments - no batch and goes to initial state
        assert!(state.is_initial());
        assert_eq!(state.context().output.len(), 0);
        with_batch(|batch| assert!(batch.is_none()));
    }

    #[tokio::test]
    async fn complex() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let batch = prepared_mock_batch_commitment(&ctx.core.db);
        let block = simple_block_data(&ctx.core.db, batch.block_hash);

        // If threshold is 1, we should not emit any events and goes thru states coordinator -> submitter -> initial
        // until batch is committed
        let (state, event) = Producer::create(ctx, block.clone(), validators.clone())
            .unwrap()
            .skip_timer()
            .await
            .unwrap()
            .process_computed_block(block.hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_commitment_submitted());

        // Check that we have a batch with commitments after submitting
        let mut ctx = state.into_context();
        with_batch(|multisigned_batch| {
            let (committed_batch, signatures) = multisigned_batch
                .cloned()
                .expect("Expected that batch is committed")
                .into_parts();

            assert_eq!(committed_batch, batch);
            assert_eq!(signatures.len(), 1);

            let (address, signature) = signatures.into_iter().next().unwrap();
            assert_eq!(
                signature
                    .validate(ctx.core.router_address, batch.to_digest())
                    .unwrap()
                    .to_address(),
                address
            );
        });

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        ctx.core.signatures_threshold = 2;
        let (state, event) = Producer::create(ctx, block.clone(), validators.clone())
            .unwrap()
            .skip_timer()
            .await
            .unwrap()
            .process_computed_block(block.hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_coordinator());
        assert!(event.is_publish_validation_request());
    }

    #[tokio::test]
    async fn code_commitments_only() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(H256::random()).prepare(&ctx.core.db, H256::random());

        let code1 = CodeCommitment::mock(()).prepare(&ctx.core.db, ());
        let code2 = CodeCommitment::mock(()).prepare(&ctx.core.db, ());
        ctx.core
            .db
            .set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());
        ctx.core.db.mutate_block_meta(block.hash, |meta| {
            meta.last_committed_batch = Some(Digest::random());
            meta.last_committed_head = Some(H256::random());
        });

        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap();

        let initial = submitter.wait_for_event().await.unwrap().0;
        assert!(initial.is_initial());
        with_batch(|batch| {
            let batch = batch.expect("Expected that batch is committed");
            assert_eq!(batch.signatures().len(), 1);
            assert!(batch.batch().chain_commitment.is_none());
            assert_eq!(batch.batch().code_commitments.len(), 2);
        });
    }

    #[async_trait]
    trait SkipTimer: Sized {
        async fn skip_timer(self) -> Result<Self>;
    }

    #[async_trait]
    impl SkipTimer for ValidatorState {
        async fn skip_timer(self) -> Result<Self> {
            assert!(self.is_producer(), "Works only for producer state");

            let (state, event) = self.wait_for_event().await?;
            assert!(state.is_producer());
            assert!(event.is_publish_producer_block());

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer());
            assert!(event.is_compute_producer_block());

            Ok(state)
        }
    }

    async fn create_producer_skip_timer(
        ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: NonEmpty<Address>,
    ) -> Result<(ValidatorState, ConsensusEvent, ConsensusEvent)> {
        let producer = Producer::create(ctx, block.clone(), validators)?;
        assert!(producer.is_producer());

        let (producer, publish_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(
            publish_event,
            ConsensusEvent::PublishProducerBlock(_)
        ));

        let (producer, compute_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(
            compute_event,
            ConsensusEvent::ComputeProducerBlock(_)
        ));

        Ok((producer, publish_event, compute_event))
    }
}
