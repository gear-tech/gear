// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Consensus
//!
//! Turns finalized Malachite block (MB) state transitions into on-chain batch commitments
//! posted to the Ethereum Router contract. Each Ethereum block one validator is elected
//! coordinator: it aggregates outcomes into a batch, collects threshold signatures, and
//! submits the batch; every other validator participates by re-deriving and signing it.
//! Block production and execution are out of scope (driven by Malachite and `ethexe-compute`).
//!
//! ## Role in the Stack
//!
//! `ethexe-service` is the sole consumer: it constructs a [`ValidatorService`] and polls
//! it as a [`ConsensusService`]. Inputs arrive from `ethexe-observer` (chain
//! heads), `ethexe-compute` (prepared blocks), and `ethexe-network` (validation requests
//! and replies). Commitments leave through the [`BatchCommitter`] trait into
//! `ethexe-ethereum`. State is read via the [`Database`](ethexe_db::Database) handle.
//! Connect (non-validator) nodes do not run this crate.
//!
//! ## Public API
//!
//! - [`ConsensusService`] — The crate's entire input/output surface: a
//!   `Stream<Item = Result<ConsensusEvent>> + FusedStream + Unpin + Send + 'static`. Inputs arrive through its `receive_*`
//!   methods.
//! - [`ConsensusEvent`] — Output stream items: [`PublishMessage`](ConsensusEvent::PublishMessage),
//!   [`CommitmentSubmitted`](ConsensusEvent::CommitmentSubmitted), and [`Warning`](ConsensusEvent::Warning).
//! - [`CommitmentSubmitted`] — Informational payload for a batch that landed on-chain; consumed via `Display`.
//! - [`ValidatorService`] — Concrete [`ConsensusService`] a validator node runs; built via `ValidatorService::new`.
//! - [`ValidatorConfig`] — Per-node configuration (`pub_key`, `signatures_threshold`, `router_address`, batch and delay limits).
//! - [`BatchCommitter`] — Trait abstracting submission of a signed batch to the Router; implemented by the `ethexe-ethereum`
//!   router wrapper.
//!
//! Inputs ([`ConsensusService`] methods):
//!
//! - [`receive_new_chain_head`](ConsensusService::receive_new_chain_head) — new Ethereum chain head; discards any in-progress
//!   commitment work and restarts for the new head.
//! - [`receive_synced_block`](ConsensusService::receive_synced_block) — block data is now in the database.
//! - [`receive_prepared_block`](ConsensusService::receive_prepared_block) — block prepared (events processed).
//! - [`receive_validation_request`](ConsensusService::receive_validation_request) — validate a batch commitment.
//! - [`receive_validation_reply`](ConsensusService::receive_validation_reply) — signed reply to a coordinated batch.
//!
//! ## Invariants
//!
//! - Exactly one coordinator is elected per Ethereum block, deterministically from the block timestamp.
//! - `commitment_delay_limit` is a per-node configuration value, not a protocol constant.

use anyhow::Result;
use ethexe_common::{
    Digest, EB, HashOf, SimpleBlockData,
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
    fn receive_synced_block(&mut self, block: HashOf<EB>) -> Result<()>;

    /// Process a prepared block received
    fn receive_prepared_block(&mut self, block: HashOf<EB>) -> Result<()>;

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
