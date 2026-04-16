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
    Announce, HashOf, ProgramStates, PromisePolicy, SimpleBlockData, ValidatorsVec,
    db::BlockMetaStorageRO, gear::BatchCommitment, injected::Promise, network::ValidatorMessage,
};
use ethexe_service_utils::Timer;
use futures::{FutureExt, future::BoxFuture};
use gprimitives::H256;
use gsigner::secp256k1::Secp256k1SignerExt;
use std::task::{Context, Poll};

/// [`Producer`] is the state of the validator, which creates a new block
/// and publish it to the network. It waits for the block to be computed
/// and then switches to [`Coordinator`] state.
#[derive(Debug, Display)]
#[display("PRODUCER in {:?}", self.state)]
pub struct Producer {
    ctx: ValidatorContext,
    block: SimpleBlockData,
    validators: ValidatorsVec,
    state: State,
}

#[derive(Debug, derive_more::IsVariant)]
enum State {
    Delay {
        #[debug(skip)]
        timer: Option<Timer>,
    },
    /// Waiting for canonical-only compute to return ProgramStates.
    WaitingCanonicalComputed {
        parent_announce: HashOf<Announce>,
    },
    /// Collecting TXs against post-canonical ProgramStates.
    /// Poll timer gives TXs time to arrive before building the announce.
    ReadyForTxCollection {
        parent_announce: HashOf<Announce>,
        #[debug(skip)]
        program_states: ProgramStates,
        #[debug(skip)]
        poll_timer: Timer,
    },
    WaitingAnnounceComputed(HashOf<Announce>),
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
                // Aggregate commitment for the block and use `announce_hash` as head for chain commitment.
                // `announce_hash` is computed and included in the db already, so it's safe to use it.
                self.state = State::AggregateBatchCommitment {
                    future: self
                        .ctx
                        .core
                        .batch_manager
                        .clone()
                        .create_batch_commitment(self.block, announce_hash)
                        .boxed(),
                };

                Ok(self.into())
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

    fn process_canonical_events_computed(
        mut self,
        block_hash: H256,
        program_states: ProgramStates,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingCanonicalComputed { parent_announce }
                if block_hash == self.block.hash =>
            {
                let parent = *parent_announce;

                // Enter TX collection window. The poll timer gives TXs
                // time to arrive before building the announce.
                let mut poll_timer = Timer::new("tx-collection poll", self.ctx.core.producer_delay);
                poll_timer.start(());

                self.state = State::ReadyForTxCollection {
                    parent_announce: parent,
                    program_states,
                    poll_timer,
                };

                Ok(self.into())
            }
            _ => DefaultProcessing::canonical_events_computed(self, block_hash, program_states),
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
            State::ReadyForTxCollection { poll_timer, .. } => {
                if poll_timer.poll_unpin(cx).is_ready() {
                    // Timer fired — collect TXs and build announce.
                    // We use mem::replace to move ProgramStates out of self.state.
                    // The Delay { timer: None } placeholder is a dead state (never fires).
                    // If build_announce_with_states errors, the `?` propagates and the
                    // producer is dropped, so the placeholder is never observed.
                    let State::ReadyForTxCollection {
                        parent_announce,
                        program_states,
                        ..
                    } = std::mem::replace(&mut self.state, State::Delay { timer: None })
                    else {
                        unreachable!()
                    };

                    let state =
                        self.build_announce_with_states(parent_announce, &program_states)?;
                    return Ok((Poll::Ready(()), state));
                }
            }
            State::AggregateBatchCommitment { future } => match future.poll_unpin(cx) {
                Poll::Ready(Ok(Some(batch))) => {
                    tracing::debug!(batch.block_hash = %batch.block_hash, "Batch commitment aggregated, switch to Coordinator");
                    return Coordinator::create(self.ctx, self.validators, batch, self.block)
                        .map(|s| (Poll::Ready(()), s));
                }
                Poll::Ready(Ok(None)) => {
                    tracing::info!("No commitments - skip batch commitment");
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
        }
        .into())
    }

    /// Phase 1: Request canonical-only compute to get fresh ProgramStates for TX validation.
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

        // Phase 1: ask compute to run canonical events only (no TXs).
        // The result (ProgramStates) arrives via process_canonical_events_computed.
        self.ctx.output(ConsensusEvent::ComputeCanonicalEvents(
            self.block.hash,
            parent,
            self.ctx.core.block_gas_limit,
        ));
        self.state = State::WaitingCanonicalComputed {
            parent_announce: parent,
        };

        Ok(self.into())
    }

    /// Phase 2: Select TXs using post-canonical ProgramStates, build and gossip announce.
    fn build_announce_with_states(
        mut self,
        parent: HashOf<Announce>,
        program_states: &ProgramStates,
    ) -> Result<ValidatorState> {
        let injected_transactions = self
            .ctx
            .core
            .injected_pool
            .select_for_announce_with_states(self.block, parent, program_states)?;

        let announce = Announce {
            block_hash: self.block.hash,
            parent,
            gas_allowance: Some(self.ctx.core.block_gas_limit),
            injected_transactions,
        };

        let (announce_hash, newly_included) =
            self.ctx.core.db.include_announce(announce.clone())?;
        if !newly_included {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::*,
        validator::{PendingEvent, mock::*},
    };
    use async_trait::async_trait;
    use ethexe_common::{HashOf, db::*, gear::CodeCommitment, mock::*};
    use futures::StreamExt;
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
        let (ctx, keys, eth) = mock_validator_context();
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

        let state = state
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_state(|state| state.is_initial())
            .await
            .unwrap();

        // No commitments - no batch and goes to initial state
        assert!(state.is_initial());
        assert_eq!(state.context().output.len(), 0);
        assert!(eth.committed_batch.read().await.is_none());
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn threshold_one() {
        gear_utils::init_default_logger();

        let (ctx, keys, eth) = mock_validator_context();
        let validators: ValidatorsVec =
            nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let mut batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);

        // If threshold is 1, we should not emit any events and goes thru states coordinator -> submitter -> initial
        // until batch is committed
        let (state, announce_hash) = Producer::create(ctx, block, validators.clone())
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // Waiting for announce to be computed
        assert!(state.is_producer());

        // change head announce in the batch
        if let Some(c) = batch.chain_commitment.as_mut() {
            c.head_announce = announce_hash
        }

        // compute announce
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let mut state = state
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_state(|state| matches!(state, ValidatorState::Initial(_)))
            .await
            .unwrap();

        state.context_mut().tasks.select_next_some().await.unwrap();

        // Check that we have a batch with commitments after submitting
        let (committed_batch, signatures) = eth
            .committed_batch
            .read()
            .await
            .clone()
            .expect("Expected that batch is committed");

        assert_eq!(committed_batch, batch);
        assert_eq!(signatures.len(), 1);
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn threshold_two() {
        gear_utils::init_default_logger();

        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 2;
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);

        let (state, announce_hash) = Producer::create(ctx, block, validators)
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        assert!(state.is_producer(), "got {state:?}");

        // compute announce
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let (state, event) = state
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();

        // If threshold is 2, producer must goes to coordinator state and emit validation request
        assert!(state.is_coordinator());
        event
            .unwrap_publish_message()
            .unwrap_request_batch_validation();
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn code_commitments_only() {
        gear_utils::init_default_logger();

        let (ctx, keys, eth) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();

        let code1 = CodeCommitment::mock(());
        let code2 = CodeCommitment::mock(());
        ctx.core.db.set_code_valid(code1.id, code1.valid);
        ctx.core.db.set_code_valid(code2.id, code2.valid);
        ctx.core.db.mutate_block_meta(block.hash, |meta| {
            meta.codes_queue = Some([code1.id, code2.id].into_iter().collect())
        });

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

        let mut state = state
            .process_computed_announce(announce_hash)
            .unwrap()
            .wait_for_state(|state| matches!(state, ValidatorState::Initial(_)))
            .await
            .unwrap();

        state.context_mut().tasks.select_next_some().await.unwrap();

        let (batch, signatures) = eth
            .committed_batch
            .read()
            .await
            .clone()
            .expect("Expected that batch is committed");
        assert_eq!(signatures.len(), 1);
        assert_eq!(batch.chain_commitment, None);
        assert_eq!(batch.code_commitments.len(), 2);
    }

    // TODO: test that zero timer works as expected

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn new_head_during_canonical_compute() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let chain = BlockChain::mock(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        let state = Producer::create(ctx, block, validators).unwrap();

        // Wait for timer to fire → ComputeCanonicalEvents
        let (state, event) = state.wait_for_event().await.unwrap();
        assert!(event.is_compute_canonical_events());

        // Now in WaitingCanonicalComputed. Send a new head.
        let new_block = SimpleBlockData::mock(());
        let state = state.process_new_head(new_block).unwrap();

        // Should transition to Initial (canonical compute discarded)
        assert!(
            state.is_initial(),
            "new_head during WaitingCanonicalComputed must go to Initial, got {state}"
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn new_head_during_tx_collection() {
        let (ctx, keys, _) = mock_validator_context();
        let validators = nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let chain = BlockChain::mock(1).setup(&ctx.core.db);
        let block = chain.blocks[1].to_simple();

        let state = Producer::create(ctx, block, validators).unwrap();

        // Wait for timer to fire → ComputeCanonicalEvents
        let (state, event) = state.wait_for_event().await.unwrap();
        let (block_hash, _, _) = event.unwrap_compute_canonical_events();

        // Deliver canonical events → enters ReadyForTxCollection
        let state = state
            .process_canonical_events_computed(block_hash, ethexe_common::ProgramStates::new())
            .unwrap();
        assert!(state.is_producer());

        // Now in ReadyForTxCollection. Send a new head before the poll timer fires.
        let new_block = SimpleBlockData::mock(());
        let state = state.process_new_head(new_block).unwrap();

        // Should transition to Initial (TX collection discarded)
        assert!(
            state.is_initial(),
            "new_head during ReadyForTxCollection must go to Initial, got {state}"
        );
    }

    #[async_trait]
    trait ProducerExt: Sized {
        /// Skip the initial producer delay and complete the full two-phase announce production flow:
        /// 1. produce_announce is triggered, emitting ComputeCanonicalEvents.
        /// 2. process_canonical_events_computed is called, transitioning to ReadyForTxCollection.
        /// 3. The poll timer fires, triggering build_announce_with_states,
        ///    which emits PublishMessage and ComputeAnnounce.
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

            // Phase 1: timer fires → ComputeCanonicalEvents
            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer(), "Expected producer state, got {state}");
            assert!(
                event.is_compute_canonical_events(),
                "Expected ComputeCanonicalEvents, got {event:?}"
            );

            // Extract block_hash from the event before consuming state
            let (block_hash, _, _) = event.unwrap_compute_canonical_events();

            // Phase 2: deliver empty ProgramStates → enters ReadyForTxCollection
            let state = state.process_canonical_events_computed(
                block_hash,
                ethexe_common::ProgramStates::new(),
            )?;
            assert!(state.is_producer(), "Expected producer state, got {state}");

            // Phase 3: poll timer fires → builds announce → PublishMessage + ComputeAnnounce
            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer(), "Expected producer state, got {state}");
            assert!(
                event.is_publish_message(),
                "Expected PublishMessage, got {event:?}"
            );

            let (state, event) = state.wait_for_event().await?;
            assert!(state.is_producer(), "Expected producer state, got {state}");
            assert!(
                event.is_compute_announce(),
                "Expected ComputeAnnounce, got {event:?}"
            );

            Ok((state, event.unwrap_compute_announce().0.to_hash()))
        }
    }
}
