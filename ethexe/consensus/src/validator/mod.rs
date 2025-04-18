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

use anyhow::Result;
use async_trait::async_trait;
use derive_more::{Debug, From};
use ethexe_common::{ProducerBlock, SimpleBlockData};
use ethexe_db::Database;
use ethexe_ethereum::Ethereum;
use ethexe_observer::BlockSyncedData;
use ethexe_signer::{Address, PublicKey, SignedData, Signer};
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;
use std::{
    any::Any,
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
mod tests;

use crate::{
    utils::{
        BatchCommitmentValidationReply, BatchCommitmentValidationRequest,
        MultisignedBatchCommitment,
    },
    ConsensusEvent, ConsensusService,
};
use initial::Initial;

#[cfg(doc)]
use self::{
    coordinator::Coordinator, participant::Participant, producer::Producer, submitter::Submitter,
    subordinate::Subordinate,
};

/// The main validator service that implements the `ConsensusService` trait.
/// This service manages the validation workflow.
pub struct ValidatorService {
    inner: Option<Box<dyn ValidatorSubService>>,
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
    // TODO +_+_+: threshold should be a ratio (and maybe also a block dependent value)
    pub threshold: u64,
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
            threshold: config.threshold,
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
        update: impl FnOnce(Box<dyn ValidatorSubService>) -> Result<Box<dyn ValidatorSubService>>,
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

    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()> {
        self.update_inner(|inner| inner.process_synced_block(data))
    }

    fn receive_computed_block(&mut self, computed_block: H256) -> Result<()> {
        self.update_inner(|inner| inner.process_computed_block(computed_block))
    }

    fn receive_block_from_producer(&mut self, signed: SignedData<ProducerBlock>) -> Result<()> {
        self.update_inner(|inner| inner.process_block_from_producer(signed))
    }

    fn receive_validation_request(
        &mut self,
        signed: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()> {
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
            let mut inner = inner.poll(cx)?;

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
    ProducerBlock(SignedData<ProducerBlock>),
    /// A validation request
    ValidationRequest(SignedData<BatchCommitmentValidationRequest>),
}

/// Trait defining the interface for validator sub-services.
/// Each sub-service represents a different state in the validation state machine.
trait ValidatorSubService: fmt::Display + fmt::Debug + Any + Unpin + Send + 'static {
    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService>;
    fn context(&self) -> &ValidatorContext;
    fn context_mut(&mut self) -> &mut ValidatorContext;
    fn into_context(self: Box<Self>) -> ValidatorContext;

    fn process_new_head(
        self: Box<Self>,
        block: SimpleBlockData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::new_head(self.to_dyn(), block)
    }

    fn process_synced_block(
        self: Box<Self>,
        data: BlockSyncedData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::synced_block(self.to_dyn(), data)
    }

    fn process_computed_block(
        self: Box<Self>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::computed_block(self.to_dyn(), computed_block)
    }

    fn process_block_from_producer(
        self: Box<Self>,
        block: SignedData<ProducerBlock>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::block_from_producer(self.to_dyn(), block)
    }

    fn process_validation_request(
        self: Box<Self>,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::validation_request(self.to_dyn(), request)
    }

    fn process_validation_reply(
        self: Box<Self>,
        reply: BatchCommitmentValidationReply,
    ) -> Result<Box<dyn ValidatorSubService>> {
        DefaultProcessing::validation_reply(self.to_dyn(), reply)
    }

    fn poll(self: Box<Self>, _cx: &mut Context<'_>) -> Result<Box<dyn ValidatorSubService>> {
        Ok(self.to_dyn())
    }

    fn warning(&mut self, warning: String) {
        let warning = format!("{self} - {warning}");
        self.context_mut().warning(warning);
    }

    fn output(&mut self, event: ConsensusEvent) {
        self.context_mut().output(event);
    }
}

struct DefaultProcessing;

impl DefaultProcessing {
    fn new_head(
        s: Box<dyn ValidatorSubService>,
        block: SimpleBlockData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        Initial::create_with_chain_head(s.into_context(), block)
    }

    fn synced_block(
        mut s: Box<dyn ValidatorSubService>,
        data: BlockSyncedData,
    ) -> Result<Box<dyn ValidatorSubService>> {
        s.warning(format!("unexpected synced block: {}", data.block_hash));

        Ok(s)
    }

    fn computed_block(
        mut s: Box<dyn ValidatorSubService>,
        computed_block: H256,
    ) -> Result<Box<dyn ValidatorSubService>> {
        s.warning(format!("unexpected computed block: {computed_block}"));

        Ok(s)
    }

    fn block_from_producer(
        mut s: Box<dyn ValidatorSubService>,
        block: SignedData<ProducerBlock>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        s.warning(format!(
            "unexpected block from producer: {block:?}, saved for later."
        ));

        s.context_mut().pending(block);

        Ok(s)
    }

    fn validation_request(
        mut s: Box<dyn ValidatorSubService>,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<Box<dyn ValidatorSubService>> {
        s.warning(format!(
            "unexpected validation request: {request:?}, saved for later."
        ));

        s.context_mut().pending(request);

        Ok(s)
    }

    fn validation_reply(
        s: Box<dyn ValidatorSubService>,
        reply: BatchCommitmentValidationReply,
    ) -> Result<Box<dyn ValidatorSubService>> {
        log::trace!("Skip validation reply: {reply:?}");

        Ok(s)
    }
}

#[derive(Debug)]
struct ValidatorContext {
    slot_duration: Duration,
    threshold: u64,
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
