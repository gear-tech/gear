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

//! # Validator Consensus Service
//!
//! This module provides the core validation functionality for the Ethexe system.
//! It implements a state machine-based validator service that processes blocks,
//! handles validation requests, and manages the validation workflow.
//!
//! State transformations schema:
//! ```text
//! Initial
//!    |
//!    ├────> Producer
//!    |         └───> Coordinator
//!    |
//!    └───> Subordinate
//!              └───> Participant
//! ```
//! * [`Initial`] switches to a [`Producer`] if it's producer for an incoming block, else becomes a [`Subordinate`].
//! * [`Producer`] switches to [`Coordinator`] after producing a block and sending it to other validators.
//! * [`Subordinate`] switches to [`Participant`] after receiving a block from the producer and waiting for its local computation.
//! * [`Coordinator`] switches to [`Initial`] after receiving enough validation replies from other validators and creates submission task.
//! * [`Participant`] switches to [`Initial`] after receiving request from [`Coordinator`] and sending validation reply (or rejecting request).
//! * Each state can be interrupted by a new chain head -> switches to [`Initial`] immediately.

use crate::{
    BatchCommitmentValidationReply, ComputedAnnounce, ConsensusEvent, ConsensusService,
    VerifiedAnnounce, VerifiedValidationRequest,
    engine::{
        EngineContext,
        prelude::{DkgEngine, RoastEngine},
    },
    utils,
    validator::{
        core::{MiddlewareWrapper, ValidatorCore},
        tx_pool::InjectedTxPool,
    },
};
pub(crate) use adapters::{sign_dkg_action, sign_roast_message};
use anyhow::{Result, anyhow};
pub use core::BatchCommitter;
use derive_more::Debug;
use ethexe_common::{
    Address, SimpleBlockData, ToDigest,
    db::OnChainStorageRO,
    ecdsa::{PublicKey, SignedMessage},
    injected::SignedInjectedTransaction,
    network::{AnnouncesResponse, SignedValidatorMessage},
};
use ethexe_db::Database;
use ethexe_ethereum::middleware::ElectionProvider;
use futures::{
    Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
};
use gprimitives::H256;
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use initial::Initial;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

mod adapters;
mod coordinator;
mod core;
mod dispatcher;
mod initial;
mod participant;
mod producer;
mod state;
mod subordinate;
mod tx_pool;

pub(crate) type DkgEngineDb = DkgEngine<Database>;
pub(crate) type RoastEngineDb = RoastEngine<Database>;
pub(crate) use state::{DefaultProcessing, PendingEvent, StateHandler, ValidatorState};

#[cfg(test)]
mod mock;
#[allow(unused_imports)]
pub(crate) use crate::engine::roast::RoastMessage;

/// The main validator service that implements the `ConsensusService` trait.
/// This service manages the validation workflow.
pub struct ValidatorService {
    inner: Option<ValidatorState>,
}

/// Configuration parameters for the validator service.
pub struct ValidatorConfig {
    /// ECDSA public key of this validator
    pub pub_key: PublicKey,
    /// ECDSA multi-signature threshold
    // TODO #4637: threshold should be a ratio (and maybe also a block dependent value)
    pub signatures_threshold: u64,
    /// Duration of ethexe slot (only to identify producer for the incoming blocks)
    pub slot_duration: Duration,
    /// Block gas limit for producer to create announces
    pub block_gas_limit: u64,
    /// Delay limit for commitment
    pub commitment_delay_limit: u32,
    /// Producer delay before creating new announce after block prepared
    pub producer_delay: Duration,
    /// Address of the router contract
    pub router_address: Address,
    /// Threshold for producer to submit commitment despite of no transitions
    pub chain_deepness_threshold: u32,
}

impl ValidatorService {
    /// Creates a new validator service instance.
    ///
    /// # Arguments
    /// * `signer` - The signer used for cryptographic operations
    /// * `db` - The database instance
    /// * `config` - Configuration parameters for the validator
    ///
    /// # Returns
    /// A new `ValidatorService` instance
    pub fn new(
        signer: Signer,
        election_provider: impl Into<Box<dyn ElectionProvider>>,
        committer: impl Into<Box<dyn BatchCommitter>>,
        db: Database,
        config: ValidatorConfig,
    ) -> Result<Self> {
        let timelines = db
            .protocol_timelines()
            .ok_or_else(|| anyhow!("Protocol timelines not found in database"))?;

        let self_address = config.pub_key.to_address();

        let ctx = ValidatorContext {
            core: ValidatorCore {
                slot_duration: config.slot_duration,
                signatures_threshold: config.signatures_threshold,
                router_address: config.router_address,
                pub_key: config.pub_key,
                timelines,
                signer: signer.clone(),
                db: db.clone(),
                committer: committer.into(),
                middleware: MiddlewareWrapper::from_inner(election_provider),
                injected_pool: InjectedTxPool::new(db.clone()),
                chain_deepness_threshold: config.chain_deepness_threshold,
                block_gas_limit: config.block_gas_limit,
                commitment_delay_limit: config.commitment_delay_limit,
                producer_delay: config.producer_delay,
            },
            pending_events: VecDeque::new(),
            output: VecDeque::new(),
            tasks: Default::default(),
            dkg_engine: DkgEngine::new(db.clone(), self_address),
            roast_engine: RoastEngine::new(db, self_address),
        };

        Ok(Self {
            inner: Some(Initial::create(ctx)?),
        })
    }

    fn context(&self) -> &ValidatorContext {
        self.inner
            .as_ref()
            .unwrap_or_else(|| unreachable!("inner must be Some"))
            .context()
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        self.inner
            .as_mut()
            .unwrap_or_else(|| unreachable!("inner must be Some"))
            .context_mut()
    }

    fn update_inner(
        &mut self,
        update: impl FnOnce(ValidatorState) -> Result<ValidatorState>,
    ) -> Result<()> {
        let inner = self
            .inner
            .take()
            .unwrap_or_else(|| unreachable!("inner must be Some"));

        update(inner).map(|inner| {
            self.inner = Some(inner);
        })
    }
}

impl ConsensusService for ValidatorService {
    fn role(&self) -> String {
        format!("Validator ({:?})", self.context().core.pub_key.to_address())
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.update_inner(|inner| inner.process_new_head(block))
    }

    fn receive_synced_block(&mut self, block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_synced_block(block))
    }

    fn receive_prepared_block(&mut self, block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_prepared_block(block))
    }

    fn receive_computed_announce(&mut self, computed_data: ComputedAnnounce) -> Result<()> {
        self.update_inner(|inner| inner.process_computed_announce(computed_data))
    }

    fn receive_announce(&mut self, announce: VerifiedAnnounce) -> Result<()> {
        self.update_inner(|inner| inner.process_announce(announce))
    }

    fn receive_validation_request(&mut self, batch: VerifiedValidationRequest) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_request(batch))
    }

    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_reply(reply))
    }

    fn receive_announces_response(&mut self, response: AnnouncesResponse) -> Result<()> {
        self.update_inner(|inner| inner.process_announces_response(response))
    }

    fn receive_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()> {
        self.update_inner(|inner| inner.process_injected_transaction(tx))
    }

    fn receive_validator_message(&mut self, message: SignedValidatorMessage) -> Result<()> {
        self.update_inner(|inner| inner.process_validator_message(message))
    }

    fn receive_verified_validator_message(
        &mut self,
        message: ethexe_common::network::VerifiedValidatorMessage,
    ) -> Result<()> {
        self.update_inner(|inner| inner.process_verified_validator_message(message))
    }
}

impl Stream for ValidatorService {
    type Item = Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.update_inner(|mut inner| {
            // Waits until inner futures become pending.
            loop {
                let (poll, state) = inner.poll_next_state(cx)?;
                inner = state;
                if poll.is_pending() {
                    break;
                }
            }

            // Note: polling tasks after inner state futures is important,
            // because polling inner state can create consensus tasks.

            // Poll consensus tasks if any
            let ctx = inner.context_mut();
            if let Poll::Ready(Some(res)) = ctx.tasks.poll_next_unpin(cx) {
                ctx.output(res?);
            }

            // Drive DKG timeouts and publish any resulting messages.
            for action in ctx.dkg_engine.tick_timeouts()? {
                if let Some(msg) = sign_dkg_action(&ctx.core.signer, ctx.core.pub_key, action)? {
                    ctx.output(ConsensusEvent::BroadcastValidatorMessage(msg));
                }
            }
            // Drive ROAST timeouts and publish any resulting messages.
            for msg in ctx.roast_engine.tick_timeouts()? {
                let signed = sign_roast_message(&ctx.core.signer, ctx.core.pub_key, msg)?;
                ctx.output(ConsensusEvent::BroadcastValidatorMessage(signed));
            }

            Ok(inner)
        })?;

        self.context_mut()
            .output
            .pop_front()
            .map(|event| Poll::Ready(Some(Ok(event))))
            .unwrap_or(Poll::Pending)
    }
}

impl FusedStream for ValidatorService {
    fn is_terminated(&self) -> bool {
        false
    }
}

/// The context shared across all validator states.
#[derive(Debug)]
pub(crate) struct ValidatorContext {
    /// Core validator parameters and utilities.
    core: ValidatorCore,

    /// ## Important
    /// New events are pushed-front, in order to process the most recent event first.
    /// So, actually it is a stack.
    pending_events: VecDeque<PendingEvent>,
    /// Output events for outer services. Populates during the poll.
    output: VecDeque<ConsensusEvent>,

    /// Ongoing consensus tasks, if any.
    #[debug("{}", tasks.len())]
    tasks: FuturesUnordered<BoxFuture<'static, Result<ConsensusEvent>>>,

    /// DKG engine for distributed key generation
    dkg_engine: DkgEngineDb,
    /// ROAST engine for threshold signing
    roast_engine: RoastEngineDb,
}

impl ValidatorContext {
    pub fn output(&mut self, event: impl Into<ConsensusEvent>) {
        self.output.push_back(event.into());
    }

    pub fn pending(&mut self, event: impl Into<PendingEvent>) {
        self.pending_events.push_front(event.into());
    }

    pub fn sign_message<T: Sized + ToDigest>(&self, data: T) -> Result<SignedMessage<T>> {
        Ok(self
            .core
            .signer
            .signed_message(self.core.pub_key, data, None)?)
    }

    fn switch_to_producer_or_subordinate(
        mut self,
        block: SimpleBlockData,
    ) -> Result<ValidatorState> {
        let current_era = self.core.timelines.era_from_ts(block.header.timestamp);

        let validators = self
            .core
            .db
            .validators(current_era)
            .ok_or(anyhow!("validators not found for block({})", block.hash))?;

        // Check if we need to start DKG for this era.
        if !self.dkg_engine.is_completed(current_era)
            && self.dkg_engine.get_state(current_era).is_none()
        {
            tracing::info!(era = current_era, "🔑 Starting DKG for new era");

            // Start DKG with threshold = (2/3 * validators.len()).
            let threshold = ((validators.len() as u64 * 2) / 3).max(1) as u16;

            // Start DKG and broadcast initial round messages.
            match self
                .dkg_engine
                .handle_event(crate::engine::dkg::DkgEngineEvent::Start {
                    era: current_era,
                    validators: validators.clone().into(),
                    threshold,
                }) {
                Ok(actions) => {
                    for action in actions {
                        if let Ok(Some(msg)) =
                            sign_dkg_action(&self.core.signer, self.core.pub_key, action)
                        {
                            self.output(ConsensusEvent::BroadcastValidatorMessage(msg));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(era = current_era, "Failed to start DKG: {}", e);
                }
            }
        }

        // Determine the block producer for the current slot.
        let producer = utils::block_producer_for(
            &validators,
            block.header.timestamp,
            self.core.slot_duration.as_secs(),
        );
        let my_address = self.core.pub_key.to_address();

        if my_address == producer {
            tracing::info!(block = %block.hash, "👷 Start to work as a producer");

            producer::Producer::create(self, block, validators.clone())
        } else {
            // TODO #4636: add test (in ethexe-service) for case where is not validator for current block
            let is_validator_for_current_block = validators.contains(&my_address);

            tracing::info!(
                block = %block.hash,
                "👷 Start to work as subordinate, producer is {producer}, \
                I'm validator for current block: {is_validator_for_current_block}",
            );

            subordinate::Subordinate::create(self, block, producer, is_validator_for_current_block)
        }
    }
}

impl EngineContext for ValidatorContext {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn publish_dkg_action(&mut self, action: crate::engine::dkg::DkgAction) -> Result<()> {
        // Wrap and sign DKG actions for the validator network.
        if let Some(msg) = sign_dkg_action(&self.core.signer, self.core.pub_key, action)? {
            self.output(ConsensusEvent::BroadcastValidatorMessage(msg));
        }
        Ok(())
    }

    fn publish_roast_message(&mut self, message: crate::engine::roast::RoastMessage) -> Result<()> {
        // Wrap and sign ROAST messages for the validator network.
        let signed = sign_roast_message(&self.core.signer, self.core.pub_key, message)?;
        self.output(ConsensusEvent::BroadcastValidatorMessage(signed));
        Ok(())
    }
}
