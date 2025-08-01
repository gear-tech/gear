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
//!    |                    └───> Submitter
//!    └───> Subordinate
//!              └───> Participant
//! ```
//! * [`Initial`] switches to a [`Producer`] if it's producer for an incoming block, else becomes a [`Subordinate`].
//! * [`Producer`] switches to [`Coordinator`] after producing a block and sending it to other validators.
//! * [`Subordinate`] switches to [`Participant`] after receiving a block from the producer and waiting for its local computation.
//! * [`Coordinator`] switches to [`Submitter`] after receiving enough validation replies from other validators.
//! * [`Participant`] switches to [`Initial`] after receiving request from [`Coordinator`] and sending validation reply (or rejecting request).
//! * [`Submitter`] switches to [`Initial`] after submitting the batch commitment to the blockchain.
//! * Each state can be interrupted by a new chain head -> switches to [`Initial`] immediately.

use crate::{
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService, SignedProducerBlock,
    SignedValidationRequest,
    utils::MultisignedBatchCommitment,
    validator::{
        coordinator::Coordinator, participant::Participant, producer::Producer,
        submitter::Submitter, subordinate::Subordinate,
    },
};
use anyhow::Result;
use async_trait::async_trait;
use derive_more::{Debug, From};
use ethexe_common::{Address, SimpleBlockData, ecdsa::PublicKey};
use ethexe_db::Database;
use ethexe_ethereum::Ethereum;
use ethexe_signer::Signer;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use initial::Initial;
use std::{
    collections::VecDeque,
    fmt,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use submitter::EthereumCommitter;

mod coordinator;
mod initial;
mod participant;
mod producer;
mod submitter;
mod subordinate;

#[cfg(test)]
mod mock;

// TODO #4790: should be configurable
/// Event if chain commitment does not contain any transitions
/// and chain is not deep enough, producer still emits it to the network.
const CHAIN_DEEPNESS_THRESHOLD: u32 = 500;

// TODO #4790: should be configurable
/// Maximum chain deepness for the chain commitment aggregation.
const MAX_CHAIN_DEEPNESS: u32 = 10000;

/// The main validator service that implements the `ConsensusService` trait.
/// This service manages the validation workflow.
pub struct ValidatorService {
    inner: Option<ValidatorState>,
}

/// Configuration parameters for the validator service.
pub struct ValidatorConfig {
    /// Ethereum RPC endpoint URL
    pub ethereum_rpc: String,
    /// ECDSA public key of this validator
    pub pub_key: PublicKey,
    /// Address of the router contract
    pub router_address: Address,
    /// ECDSA multi-signature threshold
    // TODO #4637: threshold should be a ratio (and maybe also a block dependent value)
    pub signatures_threshold: u64,
    /// Duration of ethexe slot (only to identify producer for the incoming blocks)
    pub slot_duration: Duration,
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
    pub async fn new(signer: Signer, db: Database, config: ValidatorConfig) -> Result<Self> {
        let ethereum = Ethereum::new(
            &config.ethereum_rpc,
            config.router_address,
            signer.clone(),
            config.pub_key.to_address(),
        )
        .await?;

        let router = ethereum.router();

        let ctx = ValidatorContext {
            slot_duration: config.slot_duration,
            signatures_threshold: config.signatures_threshold,
            router_address: config.router_address,
            pub_key: config.pub_key,
            signer,
            db,
            committer: Box::new(EthereumCommitter { router }),
            pending_events: VecDeque::new(),
            output: VecDeque::new(),
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
        format!("Validator ({:?})", self.context().pub_key.to_address())
    }

    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()> {
        self.update_inner(|inner| inner.process_new_head(block))
    }

    fn receive_synced_block(&mut self, block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_synced_block(block))
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_computed_block(computed_block))
    }

    fn receive_block_from_producer(&mut self, signed: SignedProducerBlock) -> Result<()> {
        self.update_inner(|inner| inner.process_block_from_producer(signed))
    }

    fn receive_validation_request(&mut self, signed: SignedValidationRequest) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_request(signed))
    }

    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_reply(reply))
    }
}

impl Stream for ValidatorService {
    type Item = Result<ConsensusEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut event = None;
        self.update_inner(|inner| {
            let mut inner = inner.poll_next_state(cx)?;

            event = inner.context_mut().output.pop_front();

            Ok(inner)
        })?;

        event
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
#[derive(Clone, Debug, From, PartialEq, Eq)]
enum PendingEvent {
    /// A block from the producer
    ProducerBlock(SignedProducerBlock),
    /// A validation request
    ValidationRequest(SignedValidationRequest),
}

/// Trait defining the interface for validator inner state and events handler.
trait StateHandler
where
    Self: Sized + Into<ValidatorState> + fmt::Display,
{
    fn context(&self) -> &ValidatorContext;

    fn context_mut(&mut self) -> &mut ValidatorContext;

    fn into_context(self) -> ValidatorContext;

    fn warning(&mut self, warning: String) {
        let warning = format!("{self} - {warning}");
        self.context_mut().warning(warning);
    }

    fn output(&mut self, event: ConsensusEvent) {
        self.context_mut().output(event);
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        DefaultProcessing::new_head(self.into(), block)
    }

    fn process_synced_block(self, data: H256) -> Result<ValidatorState> {
        DefaultProcessing::synced_block(self.into(), data)
    }

    fn process_computed_block(self, computed_block: H256) -> Result<ValidatorState> {
        DefaultProcessing::computed_block(self.into(), computed_block)
    }

    fn process_block_from_producer(self, block: SignedProducerBlock) -> Result<ValidatorState> {
        DefaultProcessing::block_from_producer(self, block)
    }

    fn process_validation_request(
        self,
        request: SignedValidationRequest,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_request(self, request)
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        DefaultProcessing::validation_reply(self, reply)
    }

    fn poll_next_state(self, _cx: &mut Context<'_>) -> Result<ValidatorState> {
        Ok(self.into())
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, derive_more::Display, derive_more::From, derive_more::IsVariant)]
enum ValidatorState {
    Initial(Initial),
    Producer(Producer),
    Coordinator(Coordinator),
    Submitter(Submitter),
    Subordinate(Subordinate),
    Participant(Participant),
}

macro_rules! delegate_call {
    ($this:ident => $func:ident( $( $arg:ident ),* )) => {
        match $this {
            ValidatorState::Initial(initial) => initial.$func($( $arg ),*),
            ValidatorState::Producer(producer) => producer.$func($( $arg ),*),
            ValidatorState::Coordinator(coordinator) => coordinator.$func($( $arg ),*),
            ValidatorState::Submitter(submitter) => submitter.$func($( $arg ),*),
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

    fn warning(&mut self, warning: String) {
        delegate_call!(self => warning(warning))
    }

    fn output(&mut self, event: ConsensusEvent) {
        delegate_call!(self => output(event))
    }

    fn process_new_head(self, block: SimpleBlockData) -> Result<ValidatorState> {
        delegate_call!(self => process_new_head(block))
    }

    fn process_synced_block(self, block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_synced_block(block))
    }

    fn process_computed_block(self, computed_block: H256) -> Result<ValidatorState> {
        delegate_call!(self => process_computed_block(computed_block))
    }

    fn process_block_from_producer(self, block: SignedProducerBlock) -> Result<ValidatorState> {
        delegate_call!(self => process_block_from_producer(block))
    }

    fn process_validation_request(
        self,
        request: SignedValidationRequest,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_request(request))
    }

    fn process_validation_reply(
        self,
        reply: BatchCommitmentValidationReply,
    ) -> Result<ValidatorState> {
        delegate_call!(self => process_validation_reply(reply))
    }

    fn poll_next_state(self, cx: &mut Context<'_>) -> Result<ValidatorState> {
        delegate_call!(self => poll_next_state(cx))
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

    fn computed_block(
        s: impl Into<ValidatorState>,
        computed_block: H256,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!("unexpected computed block: {computed_block}"));
        Ok(s)
    }

    fn block_from_producer(
        s: impl Into<ValidatorState>,
        block: SignedProducerBlock,
    ) -> Result<ValidatorState> {
        let mut s = s.into();
        s.warning(format!(
            "unexpected block from producer: {block:?}, saved for later."
        ));
        s.context_mut().pending(block);
        Ok(s)
    }

    fn validation_request(
        s: impl Into<ValidatorState>,
        request: SignedValidationRequest,
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
        log::trace!("Skip validation reply: {reply:?}");
        Ok(s.into())
    }
}

#[derive(Debug)]
struct ValidatorContext {
    slot_duration: Duration,
    signatures_threshold: u64,
    router_address: Address,
    pub_key: PublicKey,

    #[debug(skip)]
    signer: Signer,
    #[debug(skip)]
    db: Database,
    #[debug(skip)]
    committer: Box<dyn BatchCommitter>,

    /// Pending events that are saved for later processing.
    ///
    /// ## Important
    /// New events are pushed-front, in order to process the most recent event first.
    /// So, actually it is a stack.
    pending_events: VecDeque<PendingEvent>,
    /// Output events for outer services. Populates during the poll.
    output: VecDeque<ConsensusEvent>,
}

impl ValidatorContext {
    pub fn warning(&mut self, warning: String) {
        self.output.push_back(ConsensusEvent::Warning(warning));
    }

    pub fn output(&mut self, event: ConsensusEvent) {
        self.output.push_back(event);
    }

    pub fn pending(&mut self, event: impl Into<PendingEvent>) {
        self.pending_events.push_front(event.into());
    }
}

/// Trait for committing batch commitments to the blockchain.
#[async_trait]
pub trait BatchCommitter: Send {
    /// Creates a boxed clone of the committer.
    fn clone_boxed(&self) -> Box<dyn BatchCommitter>;

    /// Commits a batch of signed commitments to the blockchain.
    ///
    /// # Arguments
    /// * `batch` - The batch of commitments to commit
    ///
    /// # Returns
    /// The hash of the transaction that was sent to the blockchain
    async fn commit_batch(self: Box<Self>, batch: MultisignedBatchCommitment) -> Result<H256>;
}
