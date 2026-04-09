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
    ConsensusEvent,
    announces::{self, DBAnnouncesExt},
    validator::DefaultProcessing,
};
use anyhow::{Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Announce, HashOf, PromisePolicy, SimpleBlockData, ValidatorsVec,
    db::BlockMetaStorageRO,
    gear::BatchCommitment,
    injected::{Promise, SignedInjectedTransaction},
    network::ValidatorMessage,
};
use ethexe_service_utils::Timer;
use futures::{FutureExt, future::BoxFuture};
use gsigner::secp256k1::Secp256k1SignerExt;
use std::task::{Context, Poll};

/// Maximum number of mini-announces per ETH block. Defense-in-depth against
/// TX spam. Normal operation uses 1-3. At ~400ms compute each, 30 would
/// saturate the full 12s block window.
const MAX_MINI_ANNOUNCES_PER_BLOCK: u32 = 30;

/// [`Producer`] is the state of the validator, which creates a new block
/// and publishes it to the network. After the announce is computed, it enters
/// a ready state for mini-announces until the next block arrives.
#[derive(Debug, Display)]
#[display("PRODUCER in {:?}", self.state)]
pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: ValidatorsVec,
    state: State,
    /// The next block to process after batch commitment completes.
    /// Set when `process_new_head` arrives during `ReadyForMiniAnnounce`.
    next_block: Option<SimpleBlockData>,
    /// Counter for mini-announces created in this block's cycle.
    mini_announce_count: u32,
}

#[derive(Debug, derive_more::IsVariant)]
enum State {
    Delay {
        #[debug(skip)]
        timer: Option<Timer>,
    },
    WaitingAnnounceComputed(HashOf<Announce>),
    ReadyForMiniAnnounce {
        last_announce_hash: HashOf<Announce>,
        #[debug(skip)]
        batch_timer: Timer,
    },
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

    fn process_computed_announce(
        mut self,
        announce_hash: HashOf<Announce>,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingAnnounceComputed(expected) if *expected == announce_hash => {
                // Enter ready state for mini-announces. Batch commitment will be created
                // either when the batch timer fires or when the next block arrives,
                // whichever comes first.
                let mut batch_timer = Timer::new("batch delay", self.ctx.core.producer_delay);
                batch_timer.start(());
                self.state = State::ReadyForMiniAnnounce {
                    last_announce_hash: announce_hash,
                    batch_timer,
                };

                // Drain any TXs that arrived during computation.
                self.produce_mini_announce()
            }
            State::WaitingAnnounceComputed(expected) => {
                self.warning(format!(
                    "Computed announce {} is not expected, expected {expected}",
                    announce_hash
                ));

                Ok(self.into())
            }
            _ => DefaultProcessing::computed_announce(self, announce_hash),
        }
    }

    fn process_raw_promise(
        mut self,
        promise: Promise,
        announce_hash: HashOf<Announce>,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingAnnounceComputed(expected) if *expected == announce_hash => {
                let tx_hash = promise.tx_hash;

                let signed_promise =
                    self.ctx
                        .core
                        .signer
                        .signed_message(self.ctx.core.pub_key, promise, None)?;
                self.ctx.output(signed_promise);

                tracing::trace!("consensus sign promise for transaction-hash={tx_hash}");
                Ok(self.into())
            }

            _ => DefaultProcessing::promise_for_signing(self, promise, announce_hash),
        }
    }

    fn process_injected_transaction(
        mut self,
        tx: SignedInjectedTransaction,
    ) -> Result<ValidatorState> {
        self.ctx.core.process_injected_transaction(tx)?;
        if let State::ReadyForMiniAnnounce { .. } = &self.state {
            self.produce_mini_announce()
        } else {
            Ok(self.into())
        }
    }

    fn process_new_head(mut self, block: SimpleBlockData) -> Result<ValidatorState> {
        match &self.state {
            State::ReadyForMiniAnnounce {
                last_announce_hash, ..
            } => {
                // Create batch commitment before transitioning to Initial for the new head.
                // This defers batch creation from block N's announce-compute time to block N+1's
                // arrival, but ensures the batch is still created before processing the new block.
                let last_announce_hash = *last_announce_hash;
                self.next_block = Some(block);
                self.state = State::AggregateBatchCommitment {
                    future: self
                        .ctx
                        .core
                        .batch_manager
                        .clone()
                        .create_batch_commitment(self.block, last_announce_hash)
                        .boxed(),
                };
                Ok(self.into())
            }
            State::AggregateBatchCommitment { .. } => {
                // Batch is in progress. Update next_block to the latest head
                // so we process the most recent block after batch completes.
                self.next_block = Some(block);
                Ok(self.into())
            }
            _ => {
                // TODO: if in WaitingAnnounceComputed (mid mini-announce computation),
                // batch commitment for this block is skipped. The announces are still in DB
                // and will be picked up by the next block's collect_not_committed_predecessors,
                // but block-specific code/validator/reward commitments could be missed.
                DefaultProcessing::new_head(self, block)
            }
        }
    }

    fn poll_next_state(mut self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        match &mut self.state {
            State::Delay { timer: Some(timer) } => {
                if timer.poll_unpin(cx).is_ready() {
                    let state = self.produce_announce()?;
                    return Ok((Poll::Ready(()), state));
                }
            }
            State::ReadyForMiniAnnounce {
                batch_timer,
                last_announce_hash,
            } => {
                if batch_timer.poll_unpin(cx).is_ready() {
                    // Timer fired: create batch commitment now.
                    let last_announce_hash = *last_announce_hash;
                    self.state = State::AggregateBatchCommitment {
                        future: self
                            .ctx
                            .core
                            .batch_manager
                            .clone()
                            .create_batch_commitment(self.block, last_announce_hash)
                            .boxed(),
                    };
                    return Ok((Poll::Ready(()), self.into()));
                }
            }
            State::AggregateBatchCommitment { future } => match future.poll_unpin(cx) {
                Poll::Ready(Ok(Some(batch))) => {
                    tracing::debug!(batch.block_hash = %batch.block_hash, "Batch commitment aggregated, switch to Coordinator");
                    let next_block = self.next_block.take();
                    let state = Coordinator::create(self.ctx, self.validators, batch, self.block)?;
                    // Only pass next_block if Coordinator resolved to Initial
                    // (threshold=1). For threshold>1, Coordinator needs to collect
                    // validation replies first; the next block will arrive via the
                    // service event loop when N+2 comes.
                    let state = match next_block {
                        Some(block) if state.is_initial() => state.process_new_head(block)?,
                        _ => state,
                    };
                    return Ok((Poll::Ready(()), state));
                }
                Poll::Ready(Ok(None)) => {
                    tracing::info!("No commitments - skip batch commitment");
                    let next_block = self.next_block.take();
                    let state = Initial::create(self.ctx)?;
                    let state = match next_block {
                        Some(block) => state.process_new_head(block)?,
                        None => state,
                    };
                    return Ok((Poll::Ready(()), state));
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
        validators: ValidatorsVec,
    ) -> Result<ValidatorState> {
        assert!(
            validators.contains(&ctx.core.pub_key.to_address()),
            "Producer is not in the list of validators"
        );

        let mut timer = Timer::new("producer delay", ctx.core.producer_delay);
        timer.start(());

        ctx.pending_events.clear();

        Ok(Self {
            ctx,
            block,
            validators,
            state: State::Delay { timer: Some(timer) },
            next_block: None,
            mini_announce_count: 0,
        }
        .into())
    }

    fn produce_announce(mut self) -> Result<ValidatorState> {
        if !self.ctx.core.db.block_meta(self.block.hash).prepared {
            return Err(anyhow!(
                "Impossible, block must be prepared before creating announce"
            ));
        }

        let parent = announces::best_parent_announce(
            &self.ctx.core.db,
            self.block.hash,
            self.ctx.core.commitment_delay_limit,
        )?;

        let injected_transactions = self
            .ctx
            .core
            .injected_pool
            .select_for_announce(self.block, parent)?;

        let announce = Announce {
            block_hash: self.block.hash,
            parent,
            gas_allowance: Some(self.ctx.core.block_gas_limit),
            injected_transactions,
        };

        let (announce_hash, newly_included) =
            self.ctx.core.db.include_announce(announce.clone())?;
        if !newly_included {
            // This can happen in case of abuse from rpc - the same eth block is announced multiple times,
            // then the same announce is created multiple times, and include_announce would return already included.
            // In this case we just go to initial state, without publishing anything and computing announce again.
            self.warning(format!(
                "Announce created {announce:?} is already included at {}",
                self.block.hash
            ));

            return Initial::create(self.ctx);
        }

        let era_index = self
            .ctx
            .core
            .timelines
            .era_from_ts(self.block.header.timestamp);
        let message = ValidatorMessage {
            era_index,
            payload: announce.clone(),
        };
        let message = self
            .ctx
            .core
            .signer
            .signed_data(self.ctx.core.pub_key, message, None)?;

        self.state = State::WaitingAnnounceComputed(announce_hash);
        self.ctx
            .output(ConsensusEvent::PublishMessage(message.into()));
        self.ctx.output(ConsensusEvent::ComputeAnnounce(
            announce,
            PromisePolicy::Enabled,
        ));

        Ok(self.into())
    }

    fn produce_mini_announce(mut self) -> Result<ValidatorState> {
        let State::ReadyForMiniAnnounce {
            last_announce_hash, ..
        } = &self.state
        else {
            unreachable!("produce_mini_announce called in wrong state");
        };
        let last_announce_hash = *last_announce_hash;

        if self.mini_announce_count >= MAX_MINI_ANNOUNCES_PER_BLOCK {
            tracing::warn!(
                count = self.mini_announce_count,
                "Mini-announce cap reached, deferring remaining TXs to next block"
            );
            return Ok(self.into());
        }

        let injected_transactions = self
            .ctx
            .core
            .injected_pool
            .select_for_announce(self.block, last_announce_hash)?;

        if injected_transactions.is_empty() {
            return Ok(self.into()); // stay in ReadyForMiniAnnounce
        }

        let announce = Announce {
            block_hash: self.block.hash,
            parent: last_announce_hash,
            gas_allowance: Some(self.ctx.core.block_gas_limit),
            injected_transactions,
        };

        let (announce_hash, newly_included) =
            self.ctx.core.db.include_announce(announce.clone())?;
        if !newly_included {
            return Ok(self.into());
        }

        let era_index = self
            .ctx
            .core
            .timelines
            .era_from_ts(self.block.header.timestamp);
        let message = ValidatorMessage {
            era_index,
            payload: announce.clone(),
        };
        let message = self
            .ctx
            .core
            .signer
            .signed_data(self.ctx.core.pub_key, message, None)?;

        self.mini_announce_count += 1;
        self.state = State::WaitingAnnounceComputed(announce_hash);
        self.ctx
            .output(ConsensusEvent::PublishMessage(message.into()));
        self.ctx.output(ConsensusEvent::ComputeAnnounce(
            announce,
            PromisePolicy::Enabled,
        ));

        Ok(self.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        tx_validation::MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES,
        validator::{PendingEvent, mock::*},
    };
    use async_trait::async_trait;
    use ethexe_common::{HashOf, StateHashWithQueueSize, db::*, mock::*};
    use ethexe_runtime_common::state::{Program, ProgramState, Storage};
    use gprimitives::ActorId;
    use nonempty::nonempty;

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn create() {
        let (mut ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()];
        let block = SimpleBlockData::mock(());

        ctx.pending(PendingEvent::ValidationRequest(
            ctx.core.signer.mock_verified_data(keys[0], ()),
        ));

        let producer = Producer::create(ctx, block, validators.into()).unwrap();

        let ctx = producer.context();
        assert_eq!(
            ctx.pending_events.len(),
            0,
            "Producer must ignore external events"
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn simple() {
        let (ctx, keys, _eth) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();

        let (state, announce_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // compute announce
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let state = state.process_computed_announce(announce_hash).unwrap();

        // After computed announce, producer enters ReadyForMiniAnnounce.
        // Batch commitment is deferred to the next block's cycle.
        assert!(state.is_producer());
        let producer = state.unwrap_producer();
        assert!(producer.state.is_ready_for_mini_announce());
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn mini_announce_produced() {
        gear_utils::init_default_logger();

        let (ctx, keys, _) = mock_validator_context();
        let program_id = ActorId::from([1; 32]);
        let state_hash = ctx.core.db.write_program_state(ProgramState {
            program: Program::Terminated(ActorId::from([2; 32])),
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
            ..ProgramState::zero()
        });

        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let chain = BlockChain::mock(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        let (state, announce_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // compute first announce with a known program in its state
        let mut program_states = ethexe_common::ProgramStates::new();
        program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        );
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(MockComputedAnnounceData {
                program_states,
                ..Default::default()
            }),
        }
        .setup(&state.context().core.db);

        // Mark this announce as latest computed
        state
            .context()
            .core
            .db
            .globals_mutate(|g| g.latest_computed_announce_hash = announce_hash);

        let state = state.process_computed_announce(announce_hash).unwrap();

        // Inject a TX with valid destination while in ReadyForMiniAnnounce
        let signer = gsigner::secp256k1::Signer::memory();
        let key = signer.generate().unwrap();
        let tx = signer
            .signed_message(
                key,
                ethexe_common::injected::InjectedTransaction {
                    reference_block: chain.blocks[0].hash,
                    destination: program_id,
                    ..ethexe_common::injected::InjectedTransaction::mock(())
                },
                None,
            )
            .unwrap();

        let state = state.process_injected_transaction(tx).unwrap();

        // Producer should still be a producer, now in WaitingAnnounceComputed for the mini-announce
        assert!(state.is_producer());
        let producer = state.unwrap_producer();
        assert!(
            producer.state.is_waiting_announce_computed(),
            "Expected WaitingAnnounceComputed after mini-announce, got {:?}",
            producer.state
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn mini_announce_empty_pool() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();

        let (state, announce_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let state = state.process_computed_announce(announce_hash).unwrap();

        // Inject a TX that won't pass pool selection (no valid destination)
        let signer = gsigner::secp256k1::Signer::memory();
        let key = signer.generate().unwrap();
        let tx = signer
            .signed_message(
                key,
                ethexe_common::injected::InjectedTransaction::mock(()),
                None,
            )
            .unwrap();

        let state = state.process_injected_transaction(tx).unwrap();

        // Should stay in ReadyForMiniAnnounce since pool filtered out the TX
        assert!(state.is_producer());
        let producer = state.unwrap_producer();
        assert!(
            producer.state.is_ready_for_mini_announce(),
            "Expected ReadyForMiniAnnounce when pool is empty, got {:?}",
            producer.state
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn mini_announce_chaining() {
        gear_utils::init_default_logger();

        let (ctx, keys, _) = mock_validator_context();
        let program_id = ActorId::from([1; 32]);
        let state_hash = ctx.core.db.write_program_state(ProgramState {
            program: Program::Terminated(ActorId::from([2; 32])),
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
            ..ProgramState::zero()
        });

        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let chain = BlockChain::mock(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        let (state, announce1_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // compute first announce with known program
        let mut program_states = ethexe_common::ProgramStates::new();
        program_states.insert(
            program_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        );
        AnnounceData {
            announce: state.context().core.db.announce(announce1_hash).unwrap(),
            computed: Some(MockComputedAnnounceData {
                program_states,
                ..Default::default()
            }),
        }
        .setup(&state.context().core.db);

        state
            .context()
            .core
            .db
            .globals_mutate(|g| g.latest_computed_announce_hash = announce1_hash);

        let state = state.process_computed_announce(announce1_hash).unwrap();
        assert!(state.is_producer());

        // Read first announce before injecting TX
        let first_announce = state.context().core.db.announce(announce1_hash).unwrap();

        // Now inject TX to trigger mini-announce
        let signer = gsigner::secp256k1::Signer::memory();
        let key = signer.generate().unwrap();
        let tx = signer
            .signed_message(
                key,
                ethexe_common::injected::InjectedTransaction {
                    reference_block: chain.blocks[0].hash,
                    destination: program_id,
                    ..ethexe_common::injected::InjectedTransaction::mock(())
                },
                None,
            )
            .unwrap();

        let state = state.process_injected_transaction(tx).unwrap();
        assert!(state.is_producer());

        // Find the mini-announce hash from the compute event output
        let mini2_hash = state
            .context()
            .output
            .iter()
            .find_map(|e| match e {
                ConsensusEvent::ComputeAnnounce(a, _) if a.parent == announce1_hash => {
                    Some(a.to_hash())
                }
                _ => None,
            })
            .expect("Expected a ComputeAnnounce event for mini-announce");

        // Verify the mini-announce chains to the first announce
        let mini2 = state.context().core.db.announce(mini2_hash).unwrap();
        assert_eq!(
            mini2.parent, announce1_hash,
            "mini-announce parent must be first announce"
        );
        assert_eq!(
            mini2.block_hash, first_announce.block_hash,
            "mini-announce must reference same block"
        );

        // Compute mini-announce 2
        AnnounceData {
            announce: mini2.clone(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let state = state.process_computed_announce(mini2_hash).unwrap();
        let producer = state.unwrap_producer();
        assert!(producer.state.is_ready_for_mini_announce());
        match &producer.state {
            State::ReadyForMiniAnnounce {
                last_announce_hash, ..
            } => {
                assert_eq!(
                    *last_announce_hash, mini2_hash,
                    "ready state must track mini2"
                );
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn new_head_triggers_batch_commitment() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let chain = BlockChain::mock(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        let (state, announce_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // compute announce
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let state = state.process_computed_announce(announce_hash).unwrap();
        assert!(state.is_producer());

        // New block arrives — should trigger batch commitment creation
        let new_block = SimpleBlockData::mock(());
        let state = state.process_new_head(new_block).unwrap();

        // Should still be producer, now in AggregateBatchCommitment
        assert!(state.is_producer());
        let producer = state.unwrap_producer();
        assert!(
            producer.state.is_aggregate_batch_commitment(),
            "Expected AggregateBatchCommitment after new head in ReadyForMiniAnnounce, got {:?}",
            producer.state
        );
        assert!(
            producer.next_block.is_some(),
            "next_block must be saved for processing after batch completes"
        );
    }

    // TODO: test that zero timer works as expected

    #[async_trait]
    trait ProducerExt: Sized {
        async fn skip_timer(self) -> Result<(Self, HashOf<Announce>)>;
    }

    #[async_trait]
    impl ProducerExt for ValidatorState {
        async fn skip_timer(self) -> Result<(Self, HashOf<Announce>)> {
            assert!(
                self.is_producer(),
                "Works only for producer state, got {}",
                self
            );

            let producer = self.unwrap_producer();
            assert!(
                producer.state.is_delay(),
                "Works only for waiting for codes state, got {:?}",
                producer.state
            );

            let state = ValidatorState::from(producer);

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer(), "Expected producer state, got {state}");
            assert!(event.is_publish_message());

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer(), "Expected producer state, got {state}");
            assert!(event.is_compute_announce());

            Ok((state, event.unwrap_compute_announce().0.to_hash()))
        }
    }
}
