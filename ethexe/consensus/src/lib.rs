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

//! # Ethexe Consensus
//!
//! Decides what an ethexe node should do as Ethereum blocks arrive: validate
//! announces produced by other nodes, produce announces of its own if it is
//! the producer for a block, coordinate threshold-signed batch commitments,
//! and submit those batches to the on-chain Router contract.
//!
//! Ethereum is the authoritative ledger — this crate does not invent its own
//! BFT protocol. It decides which announces to compute, collects enough
//! validator signatures on the resulting state, and posts the aggregated
//! commitment on-chain. Finality follows from the host chain.
//!
//! Two implementations of [`ConsensusService`] are provided:
//!
//! - [`ConnectService`] — a passive "connect-node" that tracks announces
//!   from producers, asks `ethexe-compute` to execute them, and requests
//!   missing announces from peers when needed. It knows the validator
//!   set (so it can tell whose announce to accept for each block), but
//!   it holds no signing key and does not submit anything on-chain.
//! - [`ValidatorService`] — an active validator. In addition to what
//!   `ConnectService` does, it produces announces when it is the
//!   producer for a block, collects validator signatures on batch
//!   commitments, and submits the multi-signed batch to the Router
//!   contract.
//!
//! Both share the same [`ConsensusService`] trait and the same
//! [`ConsensusEvent`] output stream, so `ethexe-service` can drive them
//! uniformly.
//!
//! ## Role in the stack and relation to other crates
//!
//! - `ethexe-observer` feeds Ethereum block data through
//!   [`ConsensusService::receive_new_chain_head`] and the follow-up
//!   [`ConsensusService::receive_synced_block`] notifications.
//! - `ethexe-compute` signals execution progress through
//!   [`ConsensusService::receive_prepared_block`],
//!   [`ConsensusService::receive_computed_announce`], and hands raw
//!   promises back through
//!   [`ConsensusService::receive_promise_for_signing`].
//! - `ethexe-network` delivers producer announces, validation requests
//!   and replies, fetched announces and network-forwarded injected
//!   transactions. Outgoing network messages leave as
//!   [`ConsensusEvent::PublishMessage`], [`ConsensusEvent::PublishPromise`]
//!   and [`ConsensusEvent::RequestAnnounces`].
//! - `ethexe-ethereum` is reached only from [`ValidatorService`], through
//!   the [`BatchCommitter`] trait, to submit aggregated batch
//!   commitments to the Router contract. [`ConnectService`] neither
//!   signs nor posts anything on-chain.
//! - `ethexe-service` is the sole consumer: it routes every trait call
//!   into the consensus service and routes every [`ConsensusEvent`] to
//!   the right subsystem (compute, network, logs).
//!
//! ## Entry points
//!
//! All inputs arrive through the [`ConsensusService`] trait. Outputs leave
//! through the `futures::Stream` impl that the same trait requires.
//!
//! | Trait method                                              | Meaning of the input                                                   |
//! |-----------------------------------------------------------|------------------------------------------------------------------------|
//! | [`receive_new_chain_head`](ConsensusService::receive_new_chain_head)             | A new Ethereum chain head.                                             |
//! | [`receive_synced_block`](ConsensusService::receive_synced_block)                 | The block's data is now available in the DB.                           |
//! | [`receive_prepared_block`](ConsensusService::receive_prepared_block)             | The block is now prepared.                                             |
//! | [`receive_computed_announce`](ConsensusService::receive_computed_announce)       | An announce has finished executing and its result is persisted.        |
//! | [`receive_announce`](ConsensusService::receive_announce)                         | A signed producer announce.                                            |
//! | [`receive_promise_for_signing`](ConsensusService::receive_promise_for_signing)   | A raw promise that this validator should sign.                         |
//! | [`receive_validation_request`](ConsensusService::receive_validation_request)     | A request to validate a batch commitment.                              |
//! | [`receive_validation_reply`](ConsensusService::receive_validation_reply)         | A signed reply on a batch this validator is coordinating.              |
//! | [`receive_announces_response`](ConsensusService::receive_announces_response)     | A response to a previous [`ConsensusEvent::RequestAnnounces`].         |
//! | [`receive_injected_transaction`](ConsensusService::receive_injected_transaction) | An injected transaction offered to this validator's pool.              |
//!
//! ## Output events
//!
//! | [`ConsensusEvent`]                                                                   | What it tells the service layer                                                                 |
//! |--------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------|
//! | [`AnnounceAccepted`](ConsensusEvent::AnnounceAccepted) / [`AnnounceRejected`](ConsensusEvent::AnnounceRejected) | Informational result of validating a received producer announce.                                 |
//! | [`ComputeAnnounce`](ConsensusEvent::ComputeAnnounce)                                 | The outer service must hand this announce to `ethexe-compute`, with the given `PromisePolicy`.  |
//! | [`PublishMessage`](ConsensusEvent::PublishMessage)                                   | Signed validator-to-validator message to gossip over the network.                                |
//! | [`PublishPromise`](ConsensusEvent::PublishPromise)                                   | Signed promise to gossip over the network and deliver to RPC subscribers.                        |
//! | [`RequestAnnounces`](ConsensusEvent::RequestAnnounces)                               | Ask the network to fetch announces we are missing.                                              |
//! | [`CommitmentSubmitted`](ConsensusEvent::CommitmentSubmitted)                         | Informational: a batch was successfully submitted to the Router contract.                       |
//! | [`Warning`](ConsensusEvent::Warning)                                                 | Informational: a non-fatal anomaly (unexpected input, bad reply, etc.) was detected.            |
//!
//! ## ConnectService behaviour
//!
//! `ConnectService` observes the chain. For each new Ethereum block it
//! waits until the block is synced and prepared, resolves which
//! validator is the producer for that block, and either validates the
//! producer's announce if one has already been received or keeps
//! waiting for it.
//!
//! Accepted announces turn into [`ConsensusEvent::ComputeAnnounce`]
//! with [`PromisePolicy::Disabled`](ethexe_common::PromisePolicy) —
//! observer nodes never collect promises. If any announce in the
//! ancestor chain is missing locally, the service emits
//! [`ConsensusEvent::RequestAnnounces`] and waits for the network's
//! response before proceeding.
//!
//! ## ValidatorService behaviour
//!
//! A validator runs one attempt per Ethereum block. For every new chain
//! head the service computes which validator is the producer for that
//! block and enters one of two roles. A new chain head always aborts
//! the previous attempt.
//!
//! State flow:
//!
//! ```text
//! Initial
//!   │
//!   ├── self is producer ──► Producer ───► Coordinator ───► Initial
//!   │                                      (collects replies,
//!   │                                       submits batch)
//!   │
//!   └── other producer  ──► Subordinate ─► Participant ────► Initial
//!                                          (validates the
//!                                           producer's batch,
//!                                           signs & replies)
//! ```
//!
//! These state names appear in emitted [`ConsensusEvent::Warning`]
//! messages, so they are the right handle when reading logs or tracing
//! an issue.
//!
//! Contract visible at the crate boundary:
//!
//! - The service emits exactly one [`ConsensusEvent::ComputeAnnounce`] per
//!   block it wants executed (an announce it produced itself or one it
//!   accepted from the producer). [`PromisePolicy::Enabled`](ethexe_common::PromisePolicy)
//!   is set only when this validator is the producer — only producers
//!   collect promises.
//! - When coordinating a batch, the service gossips a
//!   [`ConsensusEvent::PublishMessage`] with the validation request,
//!   collects enough [`ConsensusService::receive_validation_reply`] calls
//!   to satisfy the configured [`ValidatorConfig::signatures_threshold`],
//!   and then submits the multi-signed batch through the injected
//!   [`BatchCommitter`]. On success a [`ConsensusEvent::CommitmentSubmitted`]
//!   is emitted.
//! - When acting as participant, the service validates the incoming
//!   batch against its local state. On acceptance it publishes a signed
//!   reply over [`ConsensusEvent::PublishMessage`]; on rejection it emits
//!   a [`ConsensusEvent::Warning`] and sends nothing to the coordinator.
//! - Unexpected or malformed inputs produce [`ConsensusEvent::Warning`]
//!   rather than aborting the service.
//!
//! ## Slot and era model
//!
//! The producer for a block is a deterministic function of the validator
//! set for the block's era and the block's timestamp. Era boundaries are
//! computed from the Ethereum block timestamp relative to the genesis
//! timestamp stored in the database config (see `ProtocolTimelines`).
//! NOTE: the only wall-clock logic the crate runs is
//! [`ValidatorConfig::producer_delay`], a small pause inserted before
//! the producer starts assembling its announce; it is currently used by
//! tests and is otherwise set to zero in production.
//!
//! ## Injected transactions
//!
//! On a validator node, injected transactions are checked for standard
//! validity (not duplicated, not outdated, destination exists and is
//! initialized, etc.) and accepted ones are stored in a local pool. When
//! this validator is next the producer for a block, it drains pending
//! transactions from the pool into the announce it creates.
//! `ConnectService` ignores injected transactions entirely.
//!
//! ## When modifying this crate
//!
//! - Ethereum is the authoritative ledger. The crate
//!   only decides which announces to execute and which batches to co-sign.
//! - A new Ethereum chain head always resets the validator to `Initial`
//!   for that block. Do not introduce state carried across chain heads
//!   beyond what is already kept in the database.
//! - `ConnectService` must never sign anything or submit anything
//!   on-chain. It has no signer and no `BatchCommitter`; keep it that
//!   way.
//! - Unexpected inputs (replies from non-validators, announces from
//!   non-producers, transitions that do not match the current state) must
//!   be surfaced as [`ConsensusEvent::Warning`], not as hard errors that
//!   tear down the stream.
//! - The producer for a block must remain a pure function of on-chain
//!   data and the block timestamp. Wall-clock time must not leak into
//!   this decision (the only existing wall-clock knob is
//!   [`ValidatorConfig::producer_delay`] and it only paces when the
//!   producer acts, never who the producer is).
//! - A batch is submitted on-chain only after the number of collected
//!   signatures reaches [`ValidatorConfig::signatures_threshold`]; this
//!   is the sole trigger.

use anyhow::Result;
use ethexe_common::{
    Announce, Digest, HashOf, PromisePolicy, SimpleBlockData,
    consensus::{BatchCommitmentValidationReply, VerifiedAnnounce, VerifiedValidationRequest},
    injected::{Promise, SignedInjectedTransaction, SignedPromise},
    network::{AnnouncesRequest, AnnouncesResponse, SignedValidatorMessage},
};
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;

pub use connect::ConnectService;
pub use validator::{BatchCommitter, ValidatorConfig, ValidatorService};

mod announces;
mod connect;
mod tx_validation;
mod utils;
mod validator;

#[cfg(test)]
mod mock;

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

    /// Process a computed block received
    fn receive_computed_announce(&mut self, computed_announce: HashOf<Announce>) -> Result<()>;

    /// Process a received producer announce
    fn receive_announce(&mut self, announce: VerifiedAnnounce) -> Result<()>;

    /// Receives the raw promise for signing.
    fn receive_promise_for_signing(
        &mut self,
        promise: Promise,
        announce_hash: HashOf<Announce>,
    ) -> Result<()>;

    /// Process a received validation request
    fn receive_validation_request(&mut self, request: VerifiedValidationRequest) -> Result<()>;

    /// Process a received validation reply
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;

    /// Process a received announces data response
    fn receive_announces_response(&mut self, response: AnnouncesResponse) -> Result<()>;

    /// Process a received injected transaction from network
    fn receive_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()>;
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
    /// Announce from producer was accepted
    AnnounceAccepted(HashOf<Announce>),
    /// Announce from producer was rejected
    AnnounceRejected(HashOf<Announce>),
    /// Outer service have to compute announce
    ComputeAnnounce(Announce, PromisePolicy),
    /// Outer service have to publish signed message
    #[from]
    PublishMessage(SignedValidatorMessage),
    #[from]
    PublishPromise(SignedPromise),
    /// Outer service have to request announces
    #[from]
    RequestAnnounces(AnnouncesRequest),
    /// Informational event: commitment was successfully submitted
    #[from]
    CommitmentSubmitted(CommitmentSubmitted),
    /// Informational event: during service processing, a warning situation was detected
    Warning(String),
}
