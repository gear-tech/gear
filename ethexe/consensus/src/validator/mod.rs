// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Validator Consensus Service
//!
//! State flow:
//!
//! ```text
//! Idle
//!   ├── self == coordinator(eth_block_ts) ──► Coordinator ──► Idle
//!   └── otherwise                          ──► Participant ──► Idle
//! ```
//!
//! Coordinator: aggregates finalized MBs into a [`BatchCommitment`], gossips
//! a validation request, collects threshold-many signatures, submits.
//!
//! Participant: waits for the coordinator's request, re-derives the same
//! batch, and replies with a signature.
//!
//! Any new chain head aborts the current attempt and resets the state.

use crate::{
    BatchCommitmentValidationReply, ConsensusEvent, ConsensusService,
    validator::{
        batch::{BatchCommitmentManager, BatchLimits},
        coordinator::{Coordinator, CoordinatorBoot},
        core::{MiddlewareWrapper, ValidatorCore},
        idle::Idle,
        participant::Participant,
    },
};
use anyhow::Result;
pub use core::BatchCommitter;
use derive_more::{Debug, From};
use ethexe_common::{
    Address, SimpleBlockData, consensus::VerifiedValidationRequest, db::ConfigStorageRO,
    ecdsa::PublicKey,
};
use ethexe_db::Database;
use ethexe_ethereum::middleware::ElectionProvider;
use futures::{
    Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
};
use gprimitives::H256;
use gsigner::secp256k1::Signer;
use std::{
    collections::VecDeque,
    fmt,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

mod batch;
mod coordinator;
mod core;
mod idle;
mod participant;

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
    /// Coordinator-local: how many Ethereum blocks the resulting
    /// `BatchCommitment` stays valid past its target block. Encoded into
    /// `BatchCommitment::expiry` (u8). Set freely per-coordinator.
    pub commitment_delay_limit: std::num::NonZero<u8>,
    /// Address of the router contract
    pub router_address: Address,
    /// The maximum size of abi encoded batch commitment.
    pub batch_size_limit: u64,
    /// Delay between receiving a chain head and the coordinator beginning
    /// batch aggregation. Buys participants time to receive the same head
    /// and lets compute catch up on the latest finalized MB.
    pub coordinator_aggregation_delay: Duration,
    /// Force a checkpoint chain commitment when the producer's view of
    /// `last_advanced_eth_block` runs ahead of `last_committed_eb`
    /// by more than this many Eth blocks.
    pub uncommitted_chain_len_threshold: std::num::NonZero<u32>,
}

impl ValidatorService {
    pub fn new(
        signer: Signer,
        election_provider: impl Into<Box<dyn ElectionProvider>>,
        committer: impl Into<Box<dyn BatchCommitter>>,
        db: Database,
        config: ValidatorConfig,
    ) -> Result<Self> {
        let timelines = db.config().timelines;
        let limits = BatchLimits {
            commitment_delay_limit: config.commitment_delay_limit,
            batch_size_limit: config.batch_size_limit,
            uncommitted_chain_len_threshold: config.uncommitted_chain_len_threshold,
        };

        let middleware = MiddlewareWrapper::from_inner(election_provider);
        let batch_manager = BatchCommitmentManager::new(limits, db.clone(), middleware);

        let ctx = ValidatorContext {
            core: ValidatorCore {
                signatures_threshold: config.signatures_threshold,
                router_address: config.router_address,
                pub_key: config.pub_key,
                timelines,
                signer,
                db,
                committer: committer.into(),
                batch_manager,
                metrics: ValidatorMetrics::default(),
                commitment_delay_limit: config.commitment_delay_limit,
                coordinator_aggregation_delay: config.coordinator_aggregation_delay,
            },
            pending_events: VecDeque::new(),
            output: VecDeque::new(),
            tasks: Default::default(),
        };

        Ok(Self {
            inner: Some(Idle::create(ctx)?),
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

    fn receive_validation_request(&mut self, batch: VerifiedValidationRequest) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_request(batch))
    }

    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()> {
        self.update_inner(|inner| inner.process_validation_reply(reply))
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

    fn poll_next_state(self, _cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        Ok((Poll::Pending, self.into()))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(
    Debug, derive_more::Display, derive_more::From, derive_more::IsVariant, derive_more::Unwrap,
)]
enum ValidatorState {
    Idle(Idle),
    CoordinatorBoot(CoordinatorBoot),
    Coordinator(Coordinator),
    Participant(Participant),
}

macro_rules! delegate_call {
    ($this:ident => $func:ident( $( $arg:ident ),* )) => {
        match $this {
            ValidatorState::Idle(s) => s.$func($( $arg ),*),
            ValidatorState::CoordinatorBoot(s) => s.$func($( $arg ),*),
            ValidatorState::Coordinator(s) => s.$func($( $arg ),*),
            ValidatorState::Participant(s) => s.$func($( $arg ),*),
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

    fn poll_next_state(self, cx: &mut Context<'_>) -> Result<(Poll<()>, ValidatorState)> {
        delegate_call!(self => poll_next_state(cx))
    }
}

struct DefaultProcessing;

impl DefaultProcessing {
    fn new_head(s: impl Into<ValidatorState>, block: SimpleBlockData) -> Result<ValidatorState> {
        Idle::create_with_chain_head(s.into().into_context(), block)
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
}

impl ValidatorContext {
    pub fn output(&mut self, event: impl Into<ConsensusEvent>) {
        self.output.push_back(event.into());
    }

    pub fn pending(&mut self, event: impl Into<PendingEvent>) {
        self.pending_events.push_front(event.into());
    }
}

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_consensus")]
struct ValidatorMetrics {
    /// The last block number validator signed batch commitment for.
    pub last_signed_commitment_block_number: metrics::Gauge,
}
