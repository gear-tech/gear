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
    coordinator::{Coordinator, CoordinatorError},
    initial::Initial,
    StateHandler, ValidatorContext, ValidatorState,
};
use crate::ConsensusEvent;
use derive_more::{Debug, Display};
use ethexe_common::{
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    gear::{BatchCommitment, BlockCommitment, CodeCommitment},
    Address, CodeBlobInfo, ProducerBlock, SimpleBlockData,
};
use ethexe_service_utils::Timer;
use ethexe_signer::SignerError;
use futures::FutureExt;
use gprimitives::{CodeId, H256};
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
    WaitingBlockComputed(H256),
}

#[derive(Debug, thiserror::Error)]
pub enum ProducerError {
    #[error("cannot get from db previous committed block for computed block {0}")]
    PreviousCommittedBlockNotFound(H256),
    #[error("computed block {0} codes queue is not in storage")]
    ComputedBlockCodesQueueNotFound(H256),
    #[error("not found outcome for computed block {0}")]
    ComputedBlockOutcomeNotFound(H256),
    #[error("cannot get from db header for computed block {0}")]
    ComputedBlockHeaderNotFound(H256),
    #[error("validated code {0} blob info is not in storage")]
    ValidatedCodeBlobInfoNotFound(CodeId),
    #[error("aggregation error: {0}")]
    CommitmentsAggregation(#[from] AggregationError),

    #[error("coordinator error: {0}")]
    Coordinator(#[from] CoordinatorError),

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("initial error: {0}")]
    Initial(#[from] InitialError),
}

#[derive(Debug, thiserror::Error)]
pub enum AggregationError {
    #[error("some blocks in queue are not computed for block {0}")]
    SomeBlocksInQueueAreNotComputed(H256),

    #[error("{0}")]
    Any(#[from] anyhow::Error),
}

type Result<T> = std::result::Result<T, ProducerError>;

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
        if !matches!(&self.state, State::WaitingBlockComputed(hash) if *hash == computed_block) {
            self.warning(format!("unexpected computed block {computed_block}"));

            return Ok(self.into());
        }

        let batch = match Self::aggregate_commitments_for_block(&self.ctx, computed_block) {
            Err(ProducerError::SomeBlocksInQueueAreNotComputed(block)) => {
                self.warning(format!(
                    "block {block} in queue for block {computed_block} is not computed"
                ));

                return Ok(Initial::create(self.ctx)?);
            }
            Err(err) => return Err(err),
            Ok(Some(batch)) => batch,
            Ok(None) => return Ok(Initial::create(self.ctx)?),
        };

        Ok(Coordinator::create(self.ctx, self.validators, batch)?)
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        match &mut self.state {
            State::CollectCodes { timer } => {
                if timer.poll_unpin(cx).is_ready() {
                    self.create_producer_block()?
                }
            }
            State::WaitingBlockComputed(_) => {}
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

    fn aggregate_commitments_for_block(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Option<BatchCommitment>, AggregationError> {
        let block_commitments = Self::aggregate_block_commitments_for_block(ctx, block_hash)?;
        let code_commitments = Self::aggregate_code_commitments_for_block(ctx, block_hash)?;

        // TODO: add the appropriate functionality
        let rewards_commitments = vec![];

        Ok(
            (!block_commitments.is_empty() || !code_commitments.is_empty()).then_some(
                BatchCommitment {
                    block_commitments,
                    code_commitments,
                    rewards_commitments,
                },
            ),
        )
    }

    fn aggregate_block_commitments_for_block(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<BlockCommitment>> {
        let commitments_queue = ctx
            .db
            .block_commitment_queue(block_hash)
            .ok_or(ProducerError::ComputedBlockCodesQueueNotFound(block_hash))?;

        let mut commitments = Vec::new();

        let predecessor_block = block_hash;

        for block in commitments_queue {
            if !ctx.db.block_computed(block) {
                // This can happen when validator syncs from p2p network and skips some old blocks.
                return Err(ProducerError::CommitmentsAggregation(
                    AggregationError::SomeBlocksInQueueAreNotComputed(block),
                ));
            }

            let outcomes = ctx
                .db
                .block_outcome(block)
                .ok_or(ProducerError::ComputedBlockOutcomeNotFound(block_hash))?;

            let previous_committed_block = ctx
                .db
                .previous_not_empty_block(block)
                .ok_or(ProducerError::PreviousCommittedBlockNotFound(block))?;

            let header = ctx
                .db
                .block_header(block)
                .ok_or(ProducerError::ComputedBlockHeaderNotFound(block))?;

            commitments.push(BlockCommitment {
                hash: block,
                timestamp: header.timestamp,
                previous_committed_block,
                predecessor_block,
                transitions: outcomes,
            });
        }

        Ok(commitments)
    }

    fn aggregate_code_commitments_for_block(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>, AggregationError> {
        let codes_queue = ctx
            .db
            .block_codes_queue(block_hash)
            .ok_or(ProducerError::ComputedBlockCodesQueueNotFound(block_hash))?;

        codes_queue
            .into_iter()
            .filter_map(|id| Some((id, ctx.db.code_valid(id)?)))
            .map(|(id, valid)| {
                ctx.db.code_blob_info(id).ok_or_else(|| anyhow!()).map(
                    |CodeBlobInfo { timestamp, .. }| CodeCommitment {
                        id,
                        timestamp,
                        valid,
                    },
                )
            })
            .collect::<Result<Vec<CodeCommitment>>>()
            .map_err(Into::into)
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

        self.state = State::WaitingBlockComputed(self.block.hash);
        self.output(ConsensusEvent::PublishProducerBlock(signed_pb));
        self.output(ConsensusEvent::ComputeProducerBlock(pb));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::db::BlockMetaStorageWrite;
    use std::vec;

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys) = mock_validator_context();
        let validators = vec![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = mock_simple_block_data();

        ctx.pending(mock_validation_request(&ctx.signer, keys[0]).1);

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
        let block = mock_simple_block_data();
        prepare_mock_empty_block(&ctx.db, &block, H256::random());

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

        // [block2] <- ... <- [block1]
        let (block1_hash, block2_hash) = (H256::random(), H256::random());
        let (block1, block1_commitment) = prepare_block_commitment(
            &ctx.db,
            mock_block_commitment(block1_hash, block1_hash, block2_hash),
        );
        let (block2, block2_commitment) = prepare_block_commitment(
            &ctx.db,
            mock_block_commitment(block2_hash, block1_hash, H256::random()),
        );

        let code1 = prepare_code_commitment(&ctx.db, mock_code_commitment());
        let code2 = prepare_code_commitment(&ctx.db, mock_code_commitment());

        ctx.db
            .set_block_codes_queue(block1.hash, [code1.id, code2.id].into_iter().collect());
        ctx.db.set_block_commitment_queue(
            block1.hash,
            [block2.hash, block1.hash].into_iter().collect(),
        );

        // If threshold is 1, we should not emit any events and goes to submitter (thru coordinator)
        let submitter = create_producer_skip_timer(ctx, block1.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_block(block1.hash)
            .unwrap();
        assert!(submitter.is_submitter());
        assert_eq!(submitter.context().output.len(), 0);

        // Check that we have a batch with code commitments after submitting
        let initial = submitter.wait_for_event().await.unwrap().0;
        assert!(initial.is_initial());
        with_batch(|batch| {
            let batch = batch.expect("Expected that batch is committed");
            assert_eq!(batch.signatures().len(), 1);
            assert_eq!(
                batch.batch().block_commitments,
                vec![block2_commitment, block1_commitment]
            );
            assert_eq!(batch.batch().code_commitments, vec![code1, code2]);
        });

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        let mut ctx = initial.into_context();
        ctx.signatures_threshold = 2;
        let (coordinator, request) = create_producer_skip_timer(ctx, block1.clone(), validators)
            .await
            .unwrap()
            .0
            .process_computed_block(block1.hash)
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
        let block = mock_simple_block_data();
        prepare_mock_empty_block(&ctx.db, &block, H256::random());

        let code1 = prepare_code_commitment(&ctx.db, mock_code_commitment());
        let code2 = prepare_code_commitment(&ctx.db, mock_code_commitment());
        ctx.db
            .set_block_codes_queue(block.hash, [code1.id, code2.id].into_iter().collect());

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
            assert_eq!(batch.batch().block_commitments.len(), 0);
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
