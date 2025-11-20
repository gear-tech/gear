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
//! This crate provides controlling a behaviour of ethexe node depending on incoming blocks.
//!
//! The main components are:
//! - [`ConsensusService`]: A trait defining the core interface for consensus services
//! - [`ConsensusEvent`]: An enum representing various consensus events which have to be processed by outer services
//! - [`ConnectService`]: An implementation of consensus to run "connect-node"
//! - [`ValidatorService`]: An implementation of consensus to run "validator-node"
//!
//! The crate is organized into several modules:
//! - `connect`: Connection management functionality
//! - `validator`: Block validation services and implementations
//! - `utils`: Utility functions and shared data structures
//! - `announces`: Logic for handling announce branching and related operations

use anyhow::Result;
use ethexe_common::{
    Announce, Digest, HashOf, SimpleBlockData,
    consensus::{BatchCommitmentValidationReply, VerifiedAnnounce, VerifiedValidationRequest},
    injected::{SignedInjectedTransaction, SignedPromise},
    network::{AnnouncesRequest, CheckedAnnouncesResponse, SignedValidatorMessage},
};
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;

pub use connect::ConnectService;
pub use utils::{block_producer_for, block_producer_index};
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
    fn receive_computed_announce(&mut self, announce: HashOf<Announce>) -> Result<()>;

    /// Process a received producer block
    fn receive_announce(&mut self, block: VerifiedAnnounce) -> Result<()>;

    /// Process a received validation request
    fn receive_validation_request(&mut self, request: VerifiedValidationRequest) -> Result<()>;

    /// Process a received validation reply
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;

    /// Process a received announces data response
    fn receive_announces_response(&mut self, response: CheckedAnnouncesResponse) -> Result<()>;

    /// Process a received injected transaction from network
    fn receive_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("Commitment submitted, block_hash: {block_hash}, batch {batch_digest}, tx: {tx}")]
pub struct CommitmentSubmitted {
    block_hash: H256,
    batch_digest: Digest,
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
    #[from]
    ComputeAnnounce(Announce),
    /// Outer service have to publish signed message
    #[from]
    PublishMessage(SignedValidatorMessage),
    /// Outer service have to request announces
    #[from]
    RequestAnnounces(AnnouncesRequest),
    /// Informational event: commitment was successfully submitted
    #[from]
    CommitmentSubmitted(CommitmentSubmitted),
    /// Informational event: during service processing, a warning situation was detected
    Warning(String),
    /// Promise for [`ethexe_common::injected::InjectedTransaction`] execution.
    #[from]
    Promise(SignedPromise),
}
