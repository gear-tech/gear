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
use crate::{
    ConsensusEvent, utils,
    validator::{CHAIN_DEEPNESS_THRESHOLD, DefaultProcessing, MAX_CHAIN_DEEPNESS},
};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, Announce, AnnounceHash, SimpleBlockData,
    db::{AnnounceStorageRead, BlockMetaStorageRead},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
};
use ethexe_service_utils::Timer;
use futures::FutureExt;
use gprimitives::H256;
use nonempty::NonEmpty;
use std::task::Context;

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
    WaitingBlockPreparedAndCollectCodes {
        #[debug(skip)]
        timer: Option<Timer>,
        block_prepared: bool,
    },
    WaitingAnnounceComputed,
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

    fn process_prepared_block(mut self, block: H256) -> Result<ValidatorState> {
        if self.block.hash != block {
            return DefaultProcessing::prepared_block(self, block);
        }

        match &mut self.state {
            State::WaitingBlockPreparedAndCollectCodes {
                timer,
                block_prepared,
            } => {
                if *block_prepared {
                    self.ctx
                        .warning(format!("Block {block} is already prepared, ignoring"));
                }

                if timer.is_none() {
                    // Timer is already expired, we can create announce immediately
                    self.create_announce()?;
                } else {
                    // Timer is still running, we will create announce later
                    *block_prepared = true;
                }

                Ok(self.into())
            }
            State::WaitingAnnounceComputed => {
                self.warning(format!("Receiving {block} prepared twice or more"));

                Ok(self.into())
            }
        }
    }

    fn process_computed_announce(mut self, announce_hash: AnnounceHash) -> Result<ValidatorState> {
        let announce = self.ctx.db.announce(announce_hash).ok_or(anyhow!(
            "Computed announce {announce_hash} is not found in storage"
        ))?;
        if !matches!(&self.state, State::WaitingAnnounceComputed if self.block.hash == announce.block_hash)
        {
            self.warning(format!(
                "announce block hash {} is not expected, expected {}",
                announce.block_hash, self.block.hash
            ));

            return Ok(self.into());
        }

        let batch = match Self::aggregate_batch_commitment(&self.ctx, &self.block)? {
            Some(batch) => batch,
            None => return Initial::create(self.ctx),
        };

        Coordinator::create(self.ctx, self.validators, batch)
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        if let State::WaitingBlockPreparedAndCollectCodes {
            timer: maybe_timer,
            block_prepared,
        } = &mut self.state
            && let Some(timer) = maybe_timer
            && timer.poll_unpin(cx).is_ready()
        {
            *maybe_timer = None;
            if *block_prepared {
                // Timer is ready and block is prepared - we can create announce
                self.create_announce()?;
            }
        }

        Ok(self.into())
    }
}

impl Producer {
    pub fn create(
        mut ctx: ValidatorContext,
        block: SimpleBlockData,
        validators: NonEmpty<Address>,
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
            state: State::WaitingBlockPreparedAndCollectCodes {
                timer: Some(timer),
                block_prepared: false,
            },
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
        let head_announce = ctx
            .db
            .block_meta(block_hash)
            .announces
            .into_iter()
            .flat_map(|a| a.into_iter())
            .next()
            .ok_or_else(|| anyhow!("No announces found for {block_hash} in block meta storage"))?;

        let Some((commitment, deepness)) = utils::aggregate_chain_commitment(
            &ctx.db,
            head_announce,
            false,
            Some(MAX_CHAIN_DEEPNESS),
        )?
        else {
            return Ok(None);
        };

        if commitment.transitions.is_empty() && deepness <= CHAIN_DEEPNESS_THRESHOLD {
            // No transitions and chain is not deep enough, skip chain commitment
            Ok(None)
        } else {
            Ok(Some(commitment))
        }
    }

    fn aggregate_code_commitments(
        ctx: &ValidatorContext,
        block_hash: H256,
    ) -> Result<Vec<CodeCommitment>> {
        let queue =
            ctx.db.block_meta(block_hash).codes_queue.ok_or_else(|| {
                anyhow!("Computed block {block_hash} codes queue is not in storage")
            })?;

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

    fn create_announce(&mut self) -> Result<()> {
        if !self.ctx.db.block_meta(self.block.hash).prepared {
            unreachable!("Impossible, block must be prepared before creating announce");
        }

        let parent_announce = self
            .ctx
            .db
            .block_meta(self.block.header.parent_hash)
            .announces
            .into_iter()
            .flat_map(|meta| meta.into_iter())
            .next()
            .ok_or_else(|| anyhow!("No announces found for prepared block"))?;

        let pb = Announce {
            block_hash: self.block.hash,
            parent: parent_announce,
            gas_allowance: Some(self.ctx.block_gas_limit),
            // TODO #4639: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed_pb = self.ctx.signer.signed_data(self.ctx.pub_key, pb.clone())?;

        self.state = State::WaitingAnnounceComputed;
        self.output(ConsensusEvent::PublishAnnounce(signed_pb));
        self.output(ConsensusEvent::ComputeAnnounce(pb));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        validator::{PendingEvent, mock::*},
    };
    use ethexe_common::{AnnounceHash, Digest, ToDigest, db::*, mock::*};
    use nonempty::{NonEmpty, nonempty};

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(());

        ctx.pending(PendingEvent::ValidationRequest(
            ctx.signer.mock_signed_data(keys[0], ()),
        ));

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
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let parent = H256::random();
        let block = BlockChain::mock(1).setup(&ctx.db).blocks[1].to_simple();
        let announce_hash = ctx.db.top_announce_hash(block.hash);

        // Set parent announce
        ctx.db.mutate_block_meta(parent, |meta| {
            meta.prepared = true;
            meta.announces = Some([AnnounceHash::random()].into());
        });

        let producer = create_producer_skip_timer(ctx, block.clone(), validators)
            .await
            .unwrap()
            .0;

        // No commitments - no batch and goes to initial state
        let initial = producer.process_computed_announce(announce_hash).unwrap();
        assert!(initial.is_initial());
        assert_eq!(initial.context().output.len(), 0);
        with_batch(|batch| assert!(batch.is_none()));
    }

    #[tokio::test]
    async fn complex() {
        let (ctx, keys) = mock_validator_context();
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let batch = prepare_chain_for_batch_commitment(&ctx.db);
        let block = ctx.db.simple_block_data(batch.block_hash);
        let announce_hash = ctx.db.top_announce_hash(block.hash);

        // If threshold is 1, we should not emit any events and goes to submitter (thru coordinator)
        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_announce(announce_hash)
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
            .process_computed_announce(announce_hash)
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
        let validators = nonempty![ctx.pub_key.to_address(), keys[0].to_address()];
        let parent = H256::random();
        let block = BlockChain::mock(1).setup(&ctx.db).blocks[1].to_simple();
        let announce_hash = ctx.db.top_announce_hash(block.hash);

        ctx.db.mutate_block_meta(parent, |meta| {
            meta.prepared = true;
            meta.announces = Some([AnnounceHash::random()].into());
        });

        let code1 = CodeCommitment::mock(());
        let code2 = CodeCommitment::mock(());
        ctx.db.set_code_valid(code1.id, code1.valid);
        ctx.db.set_code_valid(code2.id, code2.valid);
        ctx.db.mutate_block_meta(block.hash, |meta| {
            meta.codes_queue = Some([code1.id, code2.id].into_iter().collect())
        });
        ctx.db.mutate_block_meta(block.hash, |meta| {
            meta.last_committed_batch = Some(Digest::random());
            meta.last_committed_announce = Some(AnnounceHash::random());
        });

        let submitter = create_producer_skip_timer(ctx, block.clone(), validators.clone())
            .await
            .unwrap()
            .0
            .process_computed_announce(announce_hash)
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
        validators: NonEmpty<Address>,
    ) -> Result<(ValidatorState, ConsensusEvent, ConsensusEvent)> {
        let producer = Producer::create(ctx, block.clone(), validators)?;
        assert!(producer.is_producer());

        let producer = producer.process_prepared_block(block.hash)?;
        assert!(producer.is_producer());

        let (producer, publish_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(publish_event, ConsensusEvent::PublishAnnounce(_)));

        let (producer, compute_event) = producer.wait_for_event().await?;
        assert!(producer.is_producer());
        assert!(matches!(compute_event, ConsensusEvent::ComputeAnnounce(_)));

        Ok((producer, publish_event, compute_event))
    }
}
