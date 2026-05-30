// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Consensus
//!
//! Once Malachite finalizes Sequencer Blocks (MBs) and `ethexe-compute` executes them,
//! this crate turns the resulting state transitions into on-chain batch commitments posted
//! to the Ethereum Router contract.
//!
//! ## Responsibilities
//!
//! Per Ethereum block exactly one validator is elected *coordinator* for that block
//! (deterministically from the block timestamp). The coordinator collects all MBs that
//! finalized since the last on-chain commitment, aggregates their outcomes into a batch
//! commitment, gossips a validation request, and once it has enough threshold signatures
//! pushes the batch to the Router. Every other validator is a *participant*: it waits for
//! the coordinator's request, re-derives the same batch independently, signs it if the
//! digest matches, and replies. Off-cycle both states sit in `Idle` waiting for the next
//! Ethereum chain head.
//!
//! Block production is *not* a concern of this crate — Malachite drives MB ordering and
//! `ethexe-compute` is responsible for execution. Consensus only cares about turning
//! finalized MBs into on-chain commitments.
//!
//! Connect (non-validator) nodes do not run this crate at all: their [`ConsensusService`]
//! is `None` in `ethexe-service` and they observe the chain and execute MBs locally.
//!
//! ## Role in the Stack
//!
//! - `ethexe-observer` feeds Ethereum block data through
//!   [`ConsensusService::receive_new_chain_head`] and the follow-up
//!   [`ConsensusService::receive_synced_block`] notifications.
//! - `ethexe-compute` signals progress through
//!   [`ConsensusService::receive_prepared_block`].
//! - `ethexe-network` delivers validation requests and replies.
//! - `ethexe-ethereum` is reached through the [`BatchCommitter`] trait to submit
//!   aggregated batch commitments to the Router contract.
//! - `ethexe-db` provides the [`Database`](ethexe_db::Database) handle used to read
//!   timelines, configuration, and batch data.
//! - `ethexe-service` is the sole consumer; it constructs a [`ValidatorService`] and
//!   drives it as a `Pin<Box<dyn ConsensusService>>`.
//!
//! ## Entry Points / Public API
//!
//! All inputs arrive through the [`ConsensusService`] trait. Outputs leave through its
//! `futures::Stream` impl.
//!
//! Inputs:
//!
//! - [`ConsensusService::receive_new_chain_head`] — new Ethereum chain head; always resets
//!   to `Idle`.
//! - [`ConsensusService::receive_synced_block`] — block data is now in the database.
//! - [`ConsensusService::receive_prepared_block`] — block prepared (events processed).
//! - [`ConsensusService::receive_validation_request`] — validate a batch commitment.
//! - [`ConsensusService::receive_validation_reply`] — signed reply to a coordinated batch.
//!
//! ## Key Types
//!
//! - [`ConsensusService`] — the crate's entire input/output surface; a
//!   `Stream<Item = Result<ConsensusEvent>> + FusedStream + Unpin + Send + 'static`.
//! - [`ConsensusEvent`] — output stream items:
//!   [`PublishMessage`](ConsensusEvent::PublishMessage),
//!   [`CommitmentSubmitted`](ConsensusEvent::CommitmentSubmitted), and
//!   [`Warning`](ConsensusEvent::Warning).
//! - [`CommitmentSubmitted`] — informational payload describing a batch that landed
//!   on-chain (block hash, batch digest, submission tx hash).
//! - [`ValidatorService`] — the concrete [`ConsensusService`] implementation a validator
//!   node runs; built via `ValidatorService::new`.
//! - [`ValidatorConfig`] — per-node configuration: `pub_key`, `signatures_threshold`,
//!   `commitment_delay_limit`, `router_address`, `batch_size_limit`,
//!   `coordinator_aggregation_delay`, `uncommitted_chain_len_threshold`.
//! - [`BatchCommitter`] — trait abstracting submission of a signed batch to the Router;
//!   implemented by the `ethexe-ethereum` router wrapper.
//!
//! ## State Machine
//!
//! ```text
//! Idle
//!   ├── self == coordinator(eth_block) ──► Coordinator ──► Idle
//!   └── otherwise                       ──► Participant ──► Idle
//! ```
//!
//! A new chain head always resets to `Idle`.
//!
//! ## Invariants
//!
//! - The coordinator is elected deterministically from the block timestamp; there is
//!   exactly one coordinator per Ethereum block.
//! - `commitment_delay_limit` is a per-node configuration value, not a protocol constant.
//! - A new chain head always resets the service to `Idle` regardless of its current state.

use anyhow::Result;
use ethexe_common::{
    Digest, SimpleBlockData,
    consensus::{BatchCommitmentValidationReply, VerifiedValidationRequest},
    network::SignedValidatorMessage,
};
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;

pub use validator::{BatchCommitter, ValidatorConfig, ValidatorService};

mod utils;
mod validator;

pub trait ConsensusService:
    Stream<Item = Result<ConsensusEvent>> + FusedStream + Unpin + Send + 'static
{
    /// Returns the role info of the service
    fn role(&self) -> String;

    /// Process a new chain head
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()>;

    /// Process a synced block info
    fn receive_synced_block(&mut self, block: H256) -> Result<()>;

    /// Process a prepared block received
    fn receive_prepared_block(&mut self, block: H256) -> Result<()>;

    /// Process a received validation request
    fn receive_validation_request(&mut self, request: VerifiedValidationRequest) -> Result<()>;

    /// Process a received validation reply
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("Commitment submitted, block_hash: {block_hash}, batch {batch_digest}, tx: {tx}")]
pub struct CommitmentSubmitted {
    /// Block hash for which the commitment was submitted
    block_hash: H256,
    /// Digest of the committed batch
    batch_digest: Digest,
    /// Hash of the submission transaction
    tx: H256,
}

#[derive(
    Debug, Clone, PartialEq, Eq, derive_more::From, derive_more::IsVariant, derive_more::Unwrap,
)]
pub enum ConsensusEvent {
    /// Outer service have to publish signed message
    #[from]
    PublishMessage(SignedValidatorMessage),
    /// Informational event: commitment was successfully submitted
    #[from]
    CommitmentSubmitted(CommitmentSubmitted),
    /// Informational event: during service processing, a warning situation was detected
    Warning(String),
}
