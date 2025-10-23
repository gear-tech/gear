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
use crate::{ConsensusEvent, validator::DefaultProcessing};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Address, Announce, HashOf, SimpleBlockData,
    db::{AnnounceStorageRead, BlockMetaStorageRead},
    gear::BatchCommitment,
};
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

#[derive(Debug, derive_more::IsVariant)]
enum State {
    Preparing {
        #[debug(skip)]
        codes_timer: Option<Timer>,
        block_prepared: bool,
    },
    WaitingAnnounceComputed,
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

    fn process_prepared_block(mut self, block: H256) -> Result<ValidatorState> {
        if self.block.hash != block {
            return DefaultProcessing::prepared_block(self, block);
        }

        match &mut self.state {
            State::Preparing {
                codes_timer,
                block_prepared,
            } if !*block_prepared => {
                if codes_timer.is_none() {
                    // Timer is already expired, we can create announce immediately
                    self.create_announce()?;
                } else {
                    // Timer is still running, we will create announce later
                    *block_prepared = true;
                }

                Ok(self.into())
            }
            State::Preparing { codes_timer, .. } if codes_timer.is_some() => {
                self.warning(format!("Receiving {block} prepared twice or more"));

                Ok(self.into())
            }
            State::Preparing { .. } => {
                unreachable!("Impossible, announce must be already created inside polling");
            }
            _ => DefaultProcessing::prepared_block(self, block),
        }
    }

    fn process_computed_announce(mut self, announce_hash: HashOf<Announce>) -> Result<ValidatorState> {
        let announce = self.ctx.core.db.announce(announce_hash).ok_or(anyhow!(
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
            State::Preparing {
                codes_timer: Some(timer),
                block_prepared,
            } => {
                if timer.poll_unpin(cx).is_ready() {
                    if *block_prepared {
                        // Timer is ready and block is prepared - we can create announce
                        self.create_announce()?;
                    } else {
                        self.state = State::Preparing {
                            codes_timer: None,
                            block_prepared: false,
                        }
                    }
                }
            }
            State::WaitingAnnounceComputed => {}
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
            _ => {}
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
            state: State::Preparing {
                codes_timer: Some(timer),
                block_prepared: false,
            },
        }
        .into())
    }

    fn create_announce(&mut self) -> Result<()> {
        if !self.ctx.core.db.block_meta(self.block.hash).prepared {
            return Err(anyhow!(
                "Impossible, block must be prepared before creating announce"
            ));
        }

        let parent_announce = self
            .ctx
            .core
            .db
            .block_meta(self.block.header.parent_hash)
            .announces
            .into_iter()
            .flat_map(|meta| meta.into_iter())
            .next()
            .ok_or_else(|| anyhow!("No announces found for prepared block"))?;

        let announce = Announce {
            block_hash: self.block.hash,
            parent: parent_announce,
            gas_allowance: Some(self.ctx.core.block_gas_limit),
            // TODO #4639: append off-chain transactions
            off_chain_transactions: Vec::new(),
        };

        let signed = self
            .ctx
            .core
            .signer
            .signed_data(self.ctx.core.pub_key, announce.clone())?;

        self.state = State::WaitingAnnounceComputed;
        self.output(ConsensusEvent::PublishAnnounce(signed));
        self.output(ConsensusEvent::ComputeAnnounce(announce));

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
    use async_trait::async_trait;
    use ethexe_common::{HashOf, Digest, ToDigest, db::*, gear::CodeCommitment, mock::*};
    use nonempty::nonempty;

    #[tokio::test]
    async fn create() {
        let (mut ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(());

        ctx.pending(PendingEvent::ValidationRequest(
            ctx.core.signer.mock_signed_data(keys[0], ()),
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
        let (ctx, keys, eth) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let parent = H256::random();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let announce_hash = ctx.core.db.top_announce_hash(block.hash);

        // Set parent announce
        ctx.core.db.mutate_block_meta(parent, |meta| {
            meta.prepared = true;
            meta.announces = Some([HashOf::random()].into());
        });

        let state = Producer::create(ctx, block.clone(), validators)
            .unwrap()
            .to_prepared_block_state()
            .await
            .unwrap()
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_initial()
            .await
            .unwrap();

        // No commitments - no batch and goes to initial state
        assert!(state.is_initial());
        assert_eq!(state.context().output.len(), 0);
        assert!(eth.committed_batch.lock().await.is_none());
    }

    #[tokio::test]
    async fn complex() {
        let (ctx, keys, eth) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let announce_hash = ctx.core.db.top_announce_hash(block.hash);

        // If threshold is 1, we should not emit any events and goes thru states coordinator -> submitter -> initial
        // until batch is committed
        let (state, event) = Producer::create(ctx, block.clone(), validators.clone())
            .unwrap()
            .to_prepared_block_state()
            .await
            .unwrap()
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_initial());
        assert!(event.is_commitment_submitted());

        let mut ctx = state.into_context();

        // Check that we have a batch with commitments after submitting
        let (committed_batch, signatures) = eth
            .committed_batch
            .lock()
            .await
            .clone()
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

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        ctx.core.signatures_threshold = 2;
        let (state, event) = Producer::create(ctx, block.clone(), validators.clone())
            .unwrap()
            .to_prepared_block_state()
            .await
            .unwrap()
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(state.is_coordinator());
        assert!(event.is_publish_validation_request());
    }

    #[tokio::test]
    async fn code_commitments_only() {
        let (ctx, keys, eth) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let parent = H256::random();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let announce_hash = ctx.core.db.top_announce_hash(block.hash);

        ctx.core.db.mutate_block_meta(parent, |meta| {
            meta.prepared = true;
            meta.announces = Some([HashOf::random()].into());
        });

        let code1 = CodeCommitment::mock(());
        let code2 = CodeCommitment::mock(());
        ctx.core.db.set_code_valid(code1.id, code1.valid);
        ctx.core.db.set_code_valid(code2.id, code2.valid);
        ctx.core.db.mutate_block_meta(block.hash, |meta| {
            meta.codes_queue = Some([code1.id, code2.id].into_iter().collect())
        });
        ctx.core.db.mutate_block_meta(block.hash, |meta| {
            meta.last_committed_batch = Some(Digest::random());
            meta.last_committed_announce = Some(HashOf::random());
        });

        let (state, event) = Producer::create(ctx, block.clone(), validators.clone())
            .unwrap()
            .to_prepared_block_state()
            .await
            .unwrap()
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();
        assert!(
            state.is_initial(),
            "State must go to initial, actual: {state}"
        );
        assert!(
            event.is_commitment_submitted(),
            "Event must be commitment submitted, actual: {event:?}"
        );

        let batch = eth
            .committed_batch
            .lock()
            .await
            .clone()
            .expect("Expected that batch is committed");
        assert_eq!(batch.signatures().len(), 1);
        assert!(batch.batch().chain_commitment.is_none());
        assert_eq!(batch.batch().code_commitments.len(), 2);
    }

    #[async_trait]
    trait ProducerExt: Sized {
        async fn to_prepared_block_state(self) -> Result<Self>;
    }

    #[async_trait]
    impl ProducerExt for ValidatorState {
        async fn to_prepared_block_state(self) -> Result<Self> {
            assert!(self.is_producer(), "Works only for producer state");

            let producer = self.unwrap_producer();
            assert!(
                producer.state.is_preparing(),
                "Works only for preparing state"
            );

            let block_hash = producer.block.hash;
            let state = producer.process_prepared_block(block_hash)?;

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer());
            assert!(event.is_publish_announce());

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer());
            assert!(event.is_compute_announce());

            Ok(state)
        }
    }
}
