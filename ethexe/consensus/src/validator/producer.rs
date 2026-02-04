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
use anyhow::{Context as _, Result, anyhow};
use derive_more::{Debug, Display};
use ethexe_common::{
    Announce, ComputedAnnounce, HashOf, SimpleBlockData, ValidatorsVec, db::BlockMetaStorageRO,
    gear::BatchCommitment, network::ValidatorMessage,
};
use ethexe_service_utils::Timer;
use futures::{FutureExt, future::BoxFuture};
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
        computed_data: ComputedAnnounce,
    ) -> Result<ValidatorState> {
        match &self.state {
            State::WaitingAnnounceComputed(expected)
                if *expected == computed_data.announce_hash =>
            {
                // Producer collects promises from computed announce and signs them.
                if !computed_data.promises.is_empty() {
                    let signed_promises = computed_data
                        .promises
                        .into_iter()
                        .map(|promise| {
                            self.ctx
                                .sign_message(promise)
                                .context("producer: failed to sign promise")
                        })
                        .collect::<Result<_, _>>()?;

                    self.ctx.output(ConsensusEvent::Promises(signed_promises));
                }

                // Aggregate commitments for the block into a batch.
                self.state = State::AggregateBatchCommitment {
                    future: self
                        .ctx
                        .core
                        .clone()
                        .aggregate_batch_commitment(self.block, computed_data.announce_hash)
                        .boxed(),
                };

                Ok(self.into())
            }
            State::WaitingAnnounceComputed(expected) => {
                self.warning(format!(
                    "Computed announce {} is not expected, expected {expected}",
                    computed_data.announce_hash
                ));

                Ok(self.into())
            }
            _ => DefaultProcessing::computed_announce(self, computed_data),
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
            State::AggregateBatchCommitment { future } => match future.poll_unpin(cx) {
                Poll::Ready(Ok(Some(batch))) => {
                    // A batch exists: transition to Coordinator to start ROAST signing.
                    tracing::debug!(batch.block_hash = %batch.block_hash, "Batch commitment aggregated, switch to Coordinator");
                    return Coordinator::create(self.ctx, self.validators, batch)
                        .map(|s| (Poll::Ready(()), s));
                }
                Poll::Ready(Ok(None)) => {
                    // No commitments: return to Initial state without signing.
                    tracing::info!("No commitments - skip batch commitment");
                    return Initial::create(self.ctx).map(|s| (Poll::Ready(()), s));
                }
                Poll::Ready(Err(err)) => {
                    if let Some(mismatch) =
                        err.downcast_ref::<crate::utils::ValidatorsCommitmentCountMismatch>()
                    {
                        self.warning(format!(
                            "validators commitment rejected: expected {}, got {}",
                            mismatch.package_participants, mismatch.elected_validators
                        ));
                        return Initial::create(self.ctx).map(|s| (Poll::Ready(()), s));
                    }
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

        // Producer delay throttles announce creation to avoid races.
        let mut timer = Timer::new("producer delay", ctx.core.producer_delay);
        timer.start(());

        // Producer ignores pending external events.
        ctx.pending_events.clear();

        Ok(Self {
            ctx,
            block,
            validators,
            state: State::Delay { timer: Some(timer) },
        }
        .into())
    }

    fn produce_announce(mut self) -> Result<ValidatorState> {
        if !self.ctx.core.db.block_meta(self.block.hash).prepared {
            return Err(anyhow!(
                "Impossible, block must be prepared before creating announce"
            ));
        }

        // Select the best parent announce and include any injected transactions.
        let parent = announces::best_parent_announce(
            &self.ctx.core.db,
            self.block.hash,
            self.ctx.core.commitment_delay_limit,
        )?;

        let injected_transactions = self
            .ctx
            .core
            .injected_pool
            .select_for_announce(self.block.hash, parent)?;

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

        // Sign the announce for the validator network.
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

        // Publish announce and request computation.
        self.state = State::WaitingAnnounceComputed(announce_hash);
        self.ctx
            .output(ConsensusEvent::PublishMessage(message.into()));
        self.ctx.output(ConsensusEvent::ComputeAnnounce(announce));

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
    use ethexe_common::{Address, HashOf, db::*, gear::CodeCommitment, mock::*};
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
        let validators: ValidatorsVec =
            nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
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
            .process_computed_announce(ComputedAnnounce::mock(announce_hash))
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
    async fn threshold_one_roast() {
        gear_utils::init_default_logger();

        let (ctx, keys, _eth) = mock_validator_context();
        let validators: ValidatorsVec =
            nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let era_index = ctx.core.timelines.era_from_ts(batch.timestamp);
        let validator_addrs: Vec<Address> = validators.clone().into();
        setup_test_dkg(
            &ctx.core.db,
            &validator_addrs,
            ctx.core.pub_key.to_address(),
            ctx.core.signatures_threshold as u16,
            era_index,
        )
        .unwrap();

        // With ROAST, producer always becomes coordinator and initiates threshold signing
        let (state, announce_hash) = Producer::create(ctx, block, validators.clone())
            .unwrap()
            .skip_timer()
            .await
            .unwrap();

        // Waiting for announce to be computed
        assert!(state.is_producer());

        // Set up computed announce in database before processing
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let (state, event) = state
            .process_computed_announce(ComputedAnnounce::mock(announce_hash))
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();

        // Producer transitions to Coordinator and initiates ROAST signing
        assert!(
            state.is_coordinator(),
            "Expected Coordinator state, got: {state:?}"
        );

        // With ROAST, coordinator sends BroadcastValidatorMessage (SignSessionRequest)
        assert!(
            matches!(event, ConsensusEvent::BroadcastValidatorMessage(_)),
            "Expected BroadcastValidatorMessage, got: {event:?}"
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn threshold_two_roast() {
        let (mut ctx, keys, _) = mock_validator_context();
        ctx.core.signatures_threshold = 2;
        let validators: ValidatorsVec =
            nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let era_index = ctx.core.timelines.era_from_ts(batch.timestamp);
        let validator_addrs: Vec<Address> = validators.clone().into();
        setup_test_dkg(
            &ctx.core.db,
            &validator_addrs,
            ctx.core.pub_key.to_address(),
            ctx.core.signatures_threshold as u16,
            era_index,
        )
        .unwrap();

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
            .process_computed_announce(ComputedAnnounce::mock(announce_hash))
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();

        // With ROAST, producer always transitions to coordinator and initiates threshold signing
        assert!(
            state.is_coordinator(),
            "Expected Coordinator state, got: {state:?}"
        );

        // With ROAST, coordinator sends BroadcastValidatorMessage (SignSessionRequest)
        assert!(
            matches!(event, ConsensusEvent::BroadcastValidatorMessage(_)),
            "Expected BroadcastValidatorMessage, got: {event:?}"
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn code_commitments_only_roast() {
        let (ctx, keys, _eth) = mock_validator_context();
        let validators: ValidatorsVec =
            nonempty![ctx.core.pub_key.to_address(), keys[0].to_address()].into();
        let block = BlockChain::mock(1).setup(&ctx.core.db).blocks[1].to_simple();
        let era_index = ctx.core.timelines.era_from_ts(block.header.timestamp);
        let validator_addrs: Vec<Address> = validators.clone().into();
        setup_test_dkg(
            &ctx.core.db,
            &validator_addrs,
            ctx.core.pub_key.to_address(),
            ctx.core.signatures_threshold as u16,
            era_index,
        )
        .unwrap();

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

        // Set up computed announce in database before processing
        AnnounceData {
            announce: state.context().core.db.announce(announce_hash).unwrap(),
            computed: Some(Default::default()),
        }
        .setup(&state.context().core.db);

        let (state, event) = state
            .process_computed_announce(ComputedAnnounce::mock(announce_hash))
            .unwrap()
            .wait_for_event()
            .await
            .unwrap();

        // With ROAST, producer transitions to coordinator
        assert!(
            state.is_coordinator(),
            "Expected Coordinator state, got: {state:?}"
        );

        // Coordinator sends ROAST SignSessionRequest
        assert!(
            matches!(event, ConsensusEvent::BroadcastValidatorMessage(_)),
            "Expected BroadcastValidatorMessage, got: {event:?}"
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

            Ok((state, event.unwrap_compute_announce().to_hash()))
        }
    }
}
