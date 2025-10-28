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
//! - [`SimpleConnectService`]: A basic implementation of "connect-node"
//! - [`ValidatorService`]: Service for handling block validation
//!
//! The crate is organized into several modules:
//! - `connect`: Connection management functionality
//! - `validator`: Block validation services and implementations
//! - `utils`: Utility functions and shared data structures

use anyhow::Result;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData,
    consensus::{BatchCommitmentValidationReply, VerifiedAnnounce, VerifiedValidationRequest},
    network::{AnnouncesRequest, CheckedAnnouncesResponse, SignedValidatorMessage},
};
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;

// pub use connect::SimpleConnectService;
pub use utils::{block_producer_for, block_producer_index};
pub use validator::{ValidatorConfig, ValidatorService};

// mod connect;
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

    /// Process a request for announces
    fn request_announces(&mut self, response: CheckedAnnouncesResponse) -> Result<()>;
}

#[derive(
    Debug, Clone, PartialEq, Eq, derive_more::From, derive_more::IsVariant, derive_more::Unwrap,
)]
pub enum ConsensusEvent {
    /// Outer service have to compute announce
    ComputeAnnounce(Announce),
    /// Outer service have to publish signed message
    PublishMessage(SignedValidatorMessage),
    /// Outer service have to request announces
    RequestAnnounces(AnnouncesRequest),
    /// Informational event: commitment was successfully submitted, tx hash is provided
    #[from(skip)]
    CommitmentSubmitted(H256),
    /// Informational event: during service processing, a warning situation was detected
    #[from(skip)]
    Warning(String),
}
