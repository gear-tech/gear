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
//! - [`connect`]: Connection management functionality
//! - [`validator`]: Block validation services and implementations
//! - [`utils`]: Utility functions and shared data structures

mod connect;
mod utils;
mod validator;

#[cfg(test)]
mod mock;

pub use connect::SimpleConnectService;
pub use utils::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
pub use validator::{ValidatorConfig, ValidatorService};

use ethexe_common::{ecdsa::SignedData, Digest, ProducerBlock, SimpleBlockData};
use ethexe_observer::BlockSyncedData;
use futures::{stream::FusedStream, Stream};
use gprimitives::H256;

#[derive(Debug, thiserror::Error)]
pub enum ConsesusError {
    // `MultisignedBatchCommitment` errors
    #[error("invalid reply digest: {0}")]
    InvalidReplyDigest(Digest),

    // `Participant` errors
    #[error("code commitment timestamps mismatch: local {local_ts}, requested: {requested_ts}")]
    CodesTimestampMismatch { local_ts: u64, requested_ts: u64 },
    #[error("block commitment timestamps mismatch: local {local_ts}, requested: {requested_ts}")]
    BlocksTimestamMismatch { local_ts: u64, requested_ts: u64 },
    #[error("code validation results mismatch: local {local}, requested: {requested}")]
    ValidationResultsMismatch { local: bool, requested: bool },
    #[error("code {0} blob info is not in storage")]
    CodeBlobInfoNotFound(CodeId),
    #[error("code {0} is not validated by this node")]
    CodeNotValidated(CodeId),
    #[error("requested block {0} is not processed by this node")]
    BlockNotComputed(H256),
    #[error("requested block {0} header wasn't found in storage")]
    BlockHeaderNotFound(H256),
    #[error("header not found for pred block: {0}")]
    PredBlockHeaderNotFound(H256),
    #[error("block {block_hash} commitment queue is not in storage")]
    BlockCommitmentQueueNotFound(H256),

    // `Producer` errors
    #[error("cannot get from db previous committed block for computed block {0}")]
    PreviousCommittedBlockNotFound(H256),
    #[error("computed block {0} codes queue is not in storage")]
    ComputedBlockCodesQueueNotFound(H256),
    #[error("not found outcome for computed block {0}")]
    ComputedBlockOutcomeNotFound(H256),
    #[error("cannot get from db header for computed block {0}")]
    ComputedBlockHeaderNotFound(H256),
    #[error("validated code {id} blob info is not in storage")]
    ValidatedCodeBlobInfoNotFound(CodeId),

}

type Result<T> = std::result::Result<T, ConsesusError>;

pub trait ConsensusService:
    Stream<Item = Result<ConsensusEvent>> + FusedStream + Unpin + Send + 'static
{
    /// Returns the role info of the service
    fn role(&self) -> String;

    /// Process a new chain head
    fn receive_new_chain_head(&mut self, block: SimpleBlockData) -> Result<()>;

    /// Process a synced block info
    fn receive_synced_block(&mut self, data: BlockSyncedData) -> Result<()>;

    /// Process a computed block received
    fn receive_computed_block(&mut self, block_hash: H256) -> Result<()>;

    /// Process a received producer block
    fn receive_block_from_producer(&mut self, block: SignedData<ProducerBlock>) -> Result<()>;

    /// Process a received validation request
    fn receive_validation_request(
        &mut self,
        request: SignedData<BatchCommitmentValidationRequest>,
    ) -> Result<()>;

    /// Process a received validation reply
    fn receive_validation_reply(&mut self, reply: BatchCommitmentValidationReply) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusEvent {
    /// Outer service have to compute block
    ComputeBlock(H256),
    /// Outer service have to compute producer block
    ComputeProducerBlock(ProducerBlock),
    /// Outer service have to publish signed producer block
    PublishProducerBlock(SignedData<ProducerBlock>),
    /// Outer service have to publish signed validation request
    PublishValidationRequest(SignedData<BatchCommitmentValidationRequest>),
    /// Outer service have to publish signed validation reply
    PublishValidationReply(BatchCommitmentValidationReply),
    /// Informational event: commitment was successfully submitted, tx hash is provided
    CommitmentSubmitted(H256),
    /// Informational event: during service processing, a warning situation was detected
    Warning(String),
}

// TODO #4553: temporary implementation, should be improved
/// Returns block producer for time slot. Next slot is the next validator in the list.
pub const fn block_producer_index(validators_amount: usize, slot: u64) -> usize {
    (slot % validators_amount as u64) as usize
}

#[test]
fn block_producer_index_calculates_correct_index() {
    let validators_amount = 5;
    let slot = 7;
    let index = crate::block_producer_index(validators_amount, slot);
    assert_eq!(index, 2);
}
