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
    coordinator::Coordinator, initial::Initial, StateHandler, ValidatorContext, ValidatorState,
};
use crate::{rewards::RewardsManager, utils, ConsensusEvent};
use anyhow::{anyhow, Result};
use derive_more::{Debug, Display};
use ethexe_common::{
    db::BlockMetaStorageRead,
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, OperatorRewardsCommitment,
        RewardsCommitment, StakerRewardsCommitment, ValidatorsCommitment,
    },
    Address, ProducerBlock, SimpleBlockData,
};
use ethexe_service_utils::Timer;
use futures::FutureExt;
use gprimitives::H256;
use std::task::Context;

/// [`Producer`] is the state of the validator, which creates a new block
/// and publish it to the network. It waits for the block to be computed
/// and then switches to [`Coordinator`] state.
#[derive(Debug, Display)]
#[display("PRODUCER in {:?}", self.state)]
pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: Vec<Address>,
    state: State,
}

#[derive(Debug)]
enum State {
    CollectCodes {
        #[debug(skip)]
        timer: Timer,
    },
    WaitingBlockComputed,
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

        let batch = match Self::aggregate_batch_commitment(&self.ctx, &self.block)? {
            Some(batch) => batch,
            None => return Initial::create(self.ctx),
        };

        Coordinator::create(self.ctx, self.validators, batch)
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        match &mut self.state {
            State::CollectCodes { timer } => {
                if timer.poll_unpin(cx).is_ready() {
                    self.create_producer_block()?
                }
            }
            State::WaitingBlockComputed => {}
        }

        Ok(self.into())
    }
}

impl Producer {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: Vec<Address>,
    ) -> Result<ValidatorState> {
        assert!(
            validators.contains(&ctx.pub_key.to_address()),
            "Producer is not in the list of validators"
        );

        let mut timer = Timer::new("collect codes", ctx.slot_duration / 6);
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

    fn aggregate_batch_commitment(
        ctx: &ValidatorContext,
        block: &SimpleBlockData,
    ) -> Result<Option<BatchCommitment>> {
        let chain_commitment = Self::aggregate_chain_commitment(ctx, block.hash)?;
        let code_commitments = Self::aggregate_code_commitments(ctx, block.hash)?;
        let validators_commitment = Self::aggregate_validators_commitment(ctx, block.hash)?;
        let rewards_commitment = Self::aggregate_rewards_commitment(ctx, block.hash)?;

        if chain_commitment.is_none()
            && code_commitments.is_empty()
            && validators_commitment.is_none()
            && rewards_commitment.is_none()
        {
            log::debug!(
                "No commitments for block {} - skip batch commitment",
                block.hash
            );
            return Ok(None);
        }

        assert!(
            validators_commitment.is_none(),
            "TODO #4741: validators commitment is not supported yet"
        );
        assert!(
            rewards_commitment.is_none(),
            "TODO #4742: rewards commitment is not supported yet"
        );

        utils::create_batch_commitment(&ctx.db, block, chain_commitment, code_commitments)
    }

    fn aggregate_chain_commitment(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Option<ChainCommitment>> {
        let waiting_blocks_queue = ctx
            .db
            .block_commitment_queue(block_hash)
            .ok_or_else(|| anyhow!("Block {block_hash} commitment queue is not in storage"))?;

        utils::aggregate_chain_commitment(&ctx.db, waiting_blocks_queue, false)
    }

    fn aggregate_code_commitments(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>> {
        let queue = ctx
            .db
            .block_codes_queue(block_hash)
            .ok_or_else(|| anyhow!("Computed block {block_hash} codes queue is not in storage"))?;

        utils::aggregate_code_commitments(&ctx.db, queue, false)
    }

    // TODO #4741
    fn aggregate_validators_commitment(
        _ctx: &ValidatorContext,
        _block_hash: H256,
    ) -> Result<Option<ValidatorsCommitment>> {
        Ok(None)
    }

    // TODO #4742
    fn aggregate_rewards_commitment(
        _ctx: &ValidatorContext,
        _block_hash: H256,
    ) -> Result<Option<RewardsCommitment>> {
        Ok(None)
    }

    fn create_producer_block(&mut self) -> Result<()> {
        let pb = ProducerBlock {
            block_hash: self.block.hash,
            // TODO #4638: set gas allowance here
            gas_allowance: None,
            // TODO #4639: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_pb = self.ctx.signer.signed_data(self.ctx.pub_key, pb.clone())?;

        self.state = State::WaitingBlockComputed;
        self.output(ConsensusEvent::PublishProducerBlock(signed_pb));
        self.output(ConsensusEvent::ComputeProducerBlock(pb));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*, SignedValidationRequest};
    use ethexe_common::{db::BlockMetaStorageWrite, Digest, ToDigest};
    use std::vec;

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys) = mock_validator_context();
        let validators = vec![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(());

        ctx.pending(SignedValidationRequest::mock((
            ctx.signer.clone(),
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
        let validators = vec![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(()).prepare(&ctx.db, ());

        let producer = create_producer_skip_timer(ctx, block.clone(), validators)
            .await
            .unwrap()
            .0;

        // No commitments - no batch and goes to initial state
        let initial = producer.process_computed_block(block.hash).unwrap();
        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 0);
        with_batch(|batch| assert!(batch.is_none()));
    }

    #[tokio::test]
    async fn complex() {
        let (ctx, keys) = mock_validator_context();
        let validators = vec![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(()).prepare(&ctx.db, ());
        let batch = prepared_mock_batch_commitment(&ctx.db, &block);

        // If threshold is 1, we should not emit any events and goes to submitter (thru coordinator)
        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap();
        assert!(submitter.is_submitter());
        assert_eq!(submitter.context().output.len(), 0);

        let initial = submitter.wait_for_event().await.unwrap().0;
        assert!(initial.is_initial());

        // Check that we have a batch with commitments after submitting
        let mut ctx = initial.into_context();
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
                    .validate(ctx.router_address, batch.to_digest())
                    .unwrap()
                    .to_address(),
                address
            );
        });

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        ctx.signatures_threshold = 2;
        let (coordinator, request) = create_producer_skip_timer(ctx, block.clone(), validators)
            .await
            .unwrap()
            .0
            .process_computed_block(block.hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(coordinator.is_coordinator());
        assert!(matches!(
            request,
            ConsensusEvent::PublishValidationRequest(_)
        ));
    }

    #[tokio::test]
    async fn code_commitments_only() {
        let (ctx, keys) = mock_validator_context();
        let validators = vec![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(()).prepare(&ctx.db, ());

        let code1 = CodeCommitment::mock(()).prepare(&ctx.db, ());
        let code2 = CodeCommitment::mock(()).prepare(&ctx.db, ());
        ctx.db
            .set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());
        ctx.db
            .set_last_committed_batch(block.hash, Digest::random());

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

    async fn create_producer_skip_timer(
        ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: Vec<Address>,
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
