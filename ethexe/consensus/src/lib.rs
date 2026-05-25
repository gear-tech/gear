// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Consensus
//!
//! Once Malachite finalizes Sequencer Blocks (MBs) and `ethexe-compute`
//! executes them, the consensus crate is what posts the resulting state
//! transitions to the Ethereum Router contract.
//!
//! Per Ethereum block exactly one validator is elected as the *coordinator*
//! for that block (deterministically from the block timestamp). The
//! coordinator collects all MBs that finalized since the last on-chain
//! commitment, aggregates their outcomes into a batch commitment, gossips
//! a validation request, and once it has enough threshold signatures pushes
//! the batch to the Router. Every other validator is a *participant*: it
//! waits for the coordinator's request, re-derives the same batch
//! independently, signs it if the digest matches, and replies. Off-cycle
//! both states sit in `Idle` waiting for the next Ethereum
//! chain head.
//!
//! Block production is *not* a concern of this crate any more — Malachite
//! drives MB ordering and `ethexe-compute` is responsible for execution.
//! Consensus only cares about turning finalized MBs into on-chain
//! commitments.
//!
//! ## Role in the stack and relation to other crates
//!
//! - `ethexe-observer` feeds Ethereum block data through
//!   [`ConsensusService::receive_new_chain_head`] and the follow-up
//!   [`ConsensusService::receive_synced_block`] notifications.
//! - `ethexe-compute` signals progress through
//!   [`ConsensusService::receive_prepared_block`].
//! - `ethexe-network` delivers validation requests/replies.
//! - `ethexe-ethereum` is reached through the [`BatchCommitter`] trait to
//!   submit aggregated batch commitments to the Router contract.
//! - `ethexe-service` is the sole consumer.
//!
//! Connect (non-validator) nodes don't run this crate at all: their
//! `ConsensusService` is `None` in `ethexe-service` and they just observe
//! the chain plus execute MBs locally.
//!
//! ## Entry points
//!
//! All inputs arrive through the [`ConsensusService`] trait. Outputs leave
//! through the `futures::Stream` impl.
//!
//! Inputs:
//!
//! - [`receive_new_chain_head`](ConsensusService::receive_new_chain_head) — new Ethereum chain head.
//! - [`receive_synced_block`](ConsensusService::receive_synced_block) — block data is now in DB.
//! - [`receive_prepared_block`](ConsensusService::receive_prepared_block) — block prepared (events processed).
//! - [`receive_validation_request`](ConsensusService::receive_validation_request) — validate a batch commitment.
//! - [`receive_validation_reply`](ConsensusService::receive_validation_reply) — signed reply to a coordinated batch.
//!
//! ## Output events
//!
//! - [`PublishMessage`](ConsensusEvent::PublishMessage) — validator-to-validator gossip.
//! - [`CommitmentSubmitted`](ConsensusEvent::CommitmentSubmitted) — a batch landed on-chain.
//! - [`Warning`](ConsensusEvent::Warning) — non-fatal anomaly.
//!
//! ## State machine
//!
//! ```text
//! Idle
//!   ├── self == coordinator(eth_block) ──► Coordinator ──► Idle
//!   └── otherwise                       ──► Participant ──► Idle
//! ```
//!
//! A new chain head always resets to `Idle`.

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
