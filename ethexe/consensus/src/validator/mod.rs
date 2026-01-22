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

pub(crate) use crate::engine::message_adapter::{sign_dkg_action, sign_roast_message};
use crate::{
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService,
    engine::prelude::{DkgEngine, DkgEngineEvent, RoastEngine, RoastEngineEvent},
    policy::is_recoverable_roast_request_error,
    validator::{
        adapters::handle_dkg_error,
        coordinator::Coordinator,
        core::{MiddlewareWrapper, ValidatorCore},
        participant::Participant,
        producer::Producer,
        subordinate::Subordinate,
        tx_pool::InjectedTxPool,
    },
};
use anyhow::{Result, anyhow};
pub use core::BatchCommitter;
use derive_more::{Debug, From};
use ethexe_common::{
    Address, ComputedAnnounce, SimpleBlockData, ToDigest,
    consensus::{VerifiedAnnounce, VerifiedValidationRequest},
    db::OnChainStorageRO,
    ecdsa::{PublicKey, SignedMessage},
    injected::SignedInjectedTransaction,
    network::{AnnouncesResponse, SignedValidatorMessage, VerifiedValidatorMessage},
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
    fmt,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

mod adapters;
mod coordinator;
mod core;
mod initial;
mod participant;
mod producer;
mod subordinate;
mod tx_pool;

pub(crate) type DkgEngineDb = DkgEngine<Database>;
pub(crate) type RoastEngineDb = RoastEngine<Database>;

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

            for action in ctx.dkg_engine.tick_timeouts()? {
                if let Some(msg) = sign_dkg_action(&ctx.core.signer, ctx.core.pub_key, action)? {
                    ctx.output(ConsensusEvent::BroadcastValidatorMessage(msg));
                }
            }
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

/// An event that can be saved for later processing.
#[derive(Clone, Debug, From, PartialEq, Eq, derive_more::IsVariant)]
enum PendingEvent {
    /// A block from the producer
    Announce(VerifiedAnnounce),
    /// A validation request
    ValidationRequest(VerifiedValidationRequest),
}

/// Trait defining the interface for validator inner state and events handler.
trait StateHandler
where
    Self: Sized + Into<ValidatorState> + fmt::Display,
{
    fn context(&self) -> &ValidatorContext;

    fn context_mut(&mut self) -> &mut ValidatorContext;

    fn into_context(self) -> ValidatorContext;

    fn warning(&mut self, warning: impl fmt::Display) {
        let warning = format!("{self} - {warning}");
        self.context_mut()
            .output
            .push_back(ConsensusEvent::Warning(warning));
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        DefaultProcessing::new_head(self.into(), block)
    }

    fn process_synced_block(self, block: H256) -> Result<ValidatorState> {
        DefaultProcessing::synced_block(self.into(), block)
    }

    fn process_prepared_block(self, block: H256) -> Result<ValidatorState> {
        DefaultProcessing::prepared_block(self.into(), block)
    }

    fn process_computed_announce(self, computed_data: ComputedAnnounce) -> Result<ValidatorState> {
        DefaultProcessing::computed_announce(self.into(), computed_data)
    }

    fn process_announce(self, announce: VerifiedAnnounce) -> Result<ValidatorState> {
        DefaultProcessing::announce_from_producer(self, announce)
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_request(self, request)
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_reply(self, reply)
    }

    fn process_verified_validator_message(
        self,
        message: ethexe_common::network::VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        DefaultProcessing::verified_validator_message(self.into(), message)
    }

    fn process_announces_response(
        self,
        _response: AnnouncesResponse,
    ) -> Result<ValidatorState> {
        DefaultProcessing::announces_response(self, _response)
    }

    fn process_injected_transaction(self, tx: SignedInjectedTransaction) -> Result<ValidatorState> {
        DefaultProcessing::injected_transaction(self, tx)
    }

    fn process_validator_message(self, message: SignedValidatorMessage) -> Result<ValidatorState> {
        DefaultProcessing::validator_message(self, message)
    }

    fn poll_next_state(self, _cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        Ok((Poll::Pending, self.into()))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(
    Debug, derive_more::Display, derive_more::From, derive_more::IsVariant, derive_more::Unwrap,
)]
enum ValidatorState {
    Initial(Initial),
    Producer(Producer),
    Coordinator(Coordinator),
    Subordinate(Subordinate),
    Participant(Participant),
}

macro_rules! delegate_call {
    ($this:ident => $func:ident( $( $arg:ident ),* )) => {
        match $this {
            ValidatorState::Initial(initial) => initial.$func($( $arg ),*),
            ValidatorState::Producer(producer) => producer.$func($( $arg ),*),
            ValidatorState::Coordinator(coordinator) => coordinator.$func($( $arg ),*),
            ValidatorState::Subordinate(subordinate) => subordinate.$func($( $arg ),*),
            ValidatorState::Participant(participant) => participant.$func($( $arg ),*),
        }
    };
}

impl StateHandler for ValidatorState {
    fn context(&self) -> &ValidatorContext {
        delegate_call!(self => context())
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        delegate_call!(self => context_mut())
    }

    fn into_context(self) -> ValidatorContext {
        delegate_call!(self => into_context())
    }

    fn warning(&mut self, warning: impl fmt::Display) {
        delegate_call!(self => warning(warning))
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        delegate_call!(self => process_new_head(block))
    }

    fn process_synced_block(self, block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_synced_block(block))
    }

    fn process_prepared_block(self, block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_prepared_block(block))
    }

    fn process_computed_announce(self, computed_data: ComputedAnnounce) -> Result<ValidatorState> {
        delegate_call!(self => process_computed_announce(computed_data))
    }

    fn process_announce(self, verified_announce: VerifiedAnnounce) -> Result<ValidatorState> {
        delegate_call!(self => process_announce(verified_announce))
    }

    fn process_validation_request(
        self,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_request(request))
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_reply(reply))
    }

    fn process_announces_response(self, response: AnnouncesResponse) -> Result<ValidatorState> {
        delegate_call!(self => process_announces_response(response))
    }

    fn process_verified_validator_message(
        self,
        message: ethexe_common::network::VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_verified_validator_message(message))
    }

    fn poll_next_state(self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        delegate_call!(self => poll_next_state(cx))
    }

    fn process_injected_transaction(self, tx: SignedInjectedTransaction) -> Result<ValidatorState> {
        delegate_call!(self => process_injected_transaction(tx))
    }

    fn process_validator_message(self, message: SignedValidatorMessage) -> Result<ValidatorState> {
        delegate_call!(self => process_validator_message(message))
    }
}

struct DefaultProcessing;

impl DefaultProcessing {
    fn new_head(s: impl Into<ValidatorState>, block: SimpleBlockData) -> Result<ValidatorState> {
        Initial::create_with_chain_head(s.into().into_context(), block)
    }

    fn synced_block(s: impl Into<ValidatorState>, block: H256) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!("unexpected synced block: {block}"));
        Ok(s)
    }

    fn prepared_block(s: impl Into<ValidatorState>, block: H256) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!("unexpected processed block: {block}"));
        Ok(s)
    }

    fn computed_announce(
        s: impl Into<ValidatorState>,
        computed_data: ComputedAnnounce,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected computed announce: {}",
            computed_data.announce_hash
        ));
        Ok(s)
    }

    fn announce_from_producer(
        s: impl Into<ValidatorState>,
        announce: VerifiedAnnounce,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected announce from producer: {announce:?}, saved for later."
        ));
        s.context_mut().pending(announce);
        Ok(s)
    }

    fn validation_request(
        s: impl Into<ValidatorState>,
        request: VerifiedValidationRequest,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected validation request: {request:?}, saved for later."
        ));
        s.context_mut().pending(request);
        Ok(s)
    }

    fn validation_reply(
        s: impl Into<ValidatorState>,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        tracing::trace!("Skip validation reply: {reply:?}");
        Ok(s.into())
    }

    fn announces_response(
        s: impl Into<ValidatorState>,
        response: AnnouncesResponse,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected announces response: {response:?}, ignored."
        ));
        Ok(s)
    }

    fn injected_transaction(
        s: impl Into<ValidatorState>,
        tx: SignedInjectedTransaction,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.context_mut().core.process_injected_transaction(tx)?;
        Ok(s)
    }

    fn validator_message(
        s: impl Into<ValidatorState>,
        message: SignedValidatorMessage,
    ) -> Result<ValidatorState> {
        Self::verified_validator_message(s, message.into_verified())
    }

    fn verified_validator_message(
        s: impl Into<ValidatorState>,
        message: VerifiedValidatorMessage,
    ) -> Result<ValidatorState> {
        let mut s = s.into();

        // Process DKG/ROAST messages
        match message {
            VerifiedValidatorMessage::DkgRound1(msg) => {
                let era = msg.data().payload.session.era;
                match s
                    .context_mut()
                    .dkg_engine
                    .handle_event(DkgEngineEvent::Round1 {
                        from: msg.address(),
                        message: Box::new(msg.data().payload.clone()),
                    }) {
                    Ok(actions) => {
                        for action in actions {
                            if let Some(msg) = sign_dkg_action(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                action,
                            )? {
                                s.context_mut()
                                    .output(ConsensusEvent::BroadcastValidatorMessage(msg));
                            }
                        }
                    }
                    Err(err) => handle_dkg_error(&mut s, era, err),
                }
            }
            VerifiedValidatorMessage::DkgRound2(msg) => {
                let era = msg.data().payload.session.era;
                match s
                    .context_mut()
                    .dkg_engine
                    .handle_event(DkgEngineEvent::Round2 {
                        from: msg.address(),
                        message: msg.data().payload.clone(),
                    }) {
                    Ok(actions) => {
                        for action in actions {
                            if let Some(msg) = sign_dkg_action(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                action,
                            )? {
                                s.context_mut()
                                    .output(ConsensusEvent::BroadcastValidatorMessage(msg));
                            }
                        }
                    }
                    Err(err) => handle_dkg_error(&mut s, era, err),
                }
            }
            VerifiedValidatorMessage::DkgRound2Culprits(msg) => {
                let era = msg.data().payload.session.era;
                match s
                    .context_mut()
                    .dkg_engine
                    .handle_event(DkgEngineEvent::Round2Culprits {
                        from: msg.address(),
                        message: msg.data().payload.clone(),
                    }) {
                    Ok(actions) => {
                        for action in actions {
                            if let Some(msg) = sign_dkg_action(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                action,
                            )? {
                                s.context_mut()
                                    .output(ConsensusEvent::BroadcastValidatorMessage(msg));
                            }
                        }
                    }
                    Err(err) => handle_dkg_error(&mut s, era, err),
                }
            }
            VerifiedValidatorMessage::DkgComplaint(msg) => {
                let era = msg.data().payload.session.era;
                match s
                    .context_mut()
                    .dkg_engine
                    .handle_event(DkgEngineEvent::Complaint {
                        from: msg.address(),
                        message: msg.data().payload.clone(),
                    }) {
                    Ok(actions) => {
                        for action in actions {
                            if let Some(msg) = sign_dkg_action(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                action,
                            )? {
                                s.context_mut()
                                    .output(ConsensusEvent::BroadcastValidatorMessage(msg));
                            }
                        }
                    }
                    Err(err) => handle_dkg_error(&mut s, era, err),
                }
            }
            VerifiedValidatorMessage::DkgJustification(msg) => {
                let era = msg.data().payload.session.era;
                match s
                    .context_mut()
                    .dkg_engine
                    .handle_event(DkgEngineEvent::Justification {
                        from: msg.address(),
                        message: msg.data().payload.clone(),
                    }) {
                    Ok(actions) => {
                        for action in actions {
                            if let Some(msg) = sign_dkg_action(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                action,
                            )? {
                                s.context_mut()
                                    .output(ConsensusEvent::BroadcastValidatorMessage(msg));
                            }
                        }
                    }
                    Err(err) => handle_dkg_error(&mut s, era, err),
                }
            }
            VerifiedValidatorMessage::SignSessionRequest(msg) => {
                let request = msg.data().payload.clone();
                let result = s.context_mut().roast_engine.handle_event(
                    RoastEngineEvent::SignSessionRequest {
                        from: msg.address(),
                        request: request.clone(),
                    },
                );
                match result {
                    Ok(messages) => {
                        for msg in messages {
                            let signed = sign_roast_message(
                                &s.context().core.signer,
                                s.context().core.pub_key,
                                msg,
                            )?;
                            s.context_mut()
                                .output(ConsensusEvent::BroadcastValidatorMessage(signed));
                        }
                    }
                    Err(err) => {
                        let era = request.session.era;
                        let recoverable = is_recoverable_roast_request_error(&err);
                        s.warning(format!("ROAST sign request failed for era {era}: {err}"));
                        if recoverable {
                            match s.context_mut().dkg_engine.restart_with(
                                era,
                                request.participants.clone(),
                                request.threshold,
                            ) {
                                Ok(actions) => {
                                    s.warning(format!(
                                        "Restarting DKG for era {era} after invalid share data"
                                    ));
                                    for action in actions {
                                        if let Some(msg) = sign_dkg_action(
                                            &s.context().core.signer,
                                            s.context().core.pub_key,
                                            action,
                                        )? {
                                            s.context_mut().output(
                                                ConsensusEvent::BroadcastValidatorMessage(msg),
                                            );
                                        }
                                    }
                                }
                                Err(restart_err) => {
                                    s.warning(format!(
                                        "Failed to restart DKG for era {era}: {restart_err}"
                                    ));
                                }
                            }
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            VerifiedValidatorMessage::SignNonceCommit(msg) => {
                let messages =
                    s.context_mut()
                        .roast_engine
                        .handle_event(RoastEngineEvent::NonceCommit {
                            commit: msg.data().payload.clone(),
                        })?;
                for msg in messages {
                    let signed = sign_roast_message(
                        &s.context().core.signer,
                        s.context().core.pub_key,
                        msg,
                    )?;
                    s.context_mut()
                        .output(ConsensusEvent::BroadcastValidatorMessage(signed));
                }
            }
            VerifiedValidatorMessage::SignNoncePackage(msg) => {
                let messages =
                    s.context_mut()
                        .roast_engine
                        .handle_event(RoastEngineEvent::NoncePackage {
                            package: msg.data().payload.clone(),
                        })?;
                for msg in messages {
                    let signed = sign_roast_message(
                        &s.context().core.signer,
                        s.context().core.pub_key,
                        msg,
                    )?;
                    s.context_mut()
                        .output(ConsensusEvent::BroadcastValidatorMessage(signed));
                }
            }
            VerifiedValidatorMessage::SignShare(msg) => {
                let messages =
                    s.context_mut()
                        .roast_engine
                        .handle_event(RoastEngineEvent::SignShare {
                            partial: msg.data().payload.clone(),
                        })?;
                for msg in messages {
                    let signed = sign_roast_message(
                        &s.context().core.signer,
                        s.context().core.pub_key,
                        msg,
                    )?;
                    s.context_mut()
                        .output(ConsensusEvent::BroadcastValidatorMessage(signed));
                }
            }
            VerifiedValidatorMessage::SignCulprits(msg) => {
                s.context_mut()
                    .roast_engine
                    .handle_event(RoastEngineEvent::SignCulprits {
                        culprits: msg.data().payload.clone(),
                    })?;
            }
            VerifiedValidatorMessage::SignAggregate(msg) => {
                let aggregate = msg.data().payload.clone();
                tracing::info!(
                    era = msg.data().era_index,
                    msg_hash = %aggregate.msg_hash,
                    "Received ROAST aggregate signature"
                );

                s.context_mut()
                    .roast_engine
                    .handle_event(RoastEngineEvent::SignAggregate {
                        aggregate: aggregate.clone(),
                    })?;

                if let ValidatorState::Coordinator(coordinator) = s {
                    if coordinator.signing_hash == aggregate.msg_hash {
                        tracing::info!(
                            block_hash = %coordinator.batch.block_hash,
                            "✅ ROAST threshold signature completed for batch"
                        );
                        return coordinator.on_signature_complete();
                    }
                    return Ok(coordinator.into());
                }
            }
            _ => {
                tracing::warn!("Unexpected validator message type received");
            }
        }

        Ok(s)
    }
}

/// The context shared across all validator states.
#[derive(Debug)]
struct ValidatorContext {
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
}
