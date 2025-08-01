// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use ethexe_common::{CodeAndIdUnchecked, events::BlockRequestEvent};
use ethexe_processor::{BlockProcessingResult, Processor, ProcessorError};
use gprimitives::{CodeId, H256};
pub use service::ComputeService;
use std::collections::HashSet;

mod compute;
mod prepare;
mod service;
mod utils;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockProcessed {
    pub block_hash: H256,
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap)]
pub enum ComputeEvent {
    RequestLoadCodes(HashSet<CodeId>),
    CodeProcessed(CodeId),
    BlockPrepared(H256),
    BlockProcessed(BlockProcessed),
}

#[derive(thiserror::Error, Debug)]
pub enum ComputeError {
    #[error("block({0}) requested to process, but it's not prepared")]
    BlockNotPrepared(H256),
    #[error("block({0}) not synced")]
    BlockNotSynced(H256),
    #[error("not found events for block({0})")]
    BlockEventsNotFound(H256),
    #[error("block header not found for synced block({0})")]
    BlockHeaderNotFound(H256),
    #[error("process code join error")]
    CodeProcessJoin(#[from] tokio::task::JoinError),
    #[error("block outcome not set for computed block({0})")]
    ParentNotFound(H256),
    #[error("code({0}) marked as validated, but not found in db")]
    ValidatedCodeNotFound(CodeId),
    #[error("codes queue nоt found for computed block({0})")]
    CodesQueueNotFound(H256),
    #[error("commitment queue not found for computed block({0})")]
    CommitmentQueueNotFound(H256),
    #[error("previous commitment not found for computed block({0})")]
    PreviousCommitmentNotFound(H256),
    #[error("last committed batch not found for computed block({0})")]
    LastCommittedBatchNotFound(H256),
    #[error("last committed head not found for computed block({0})")]
    LastCommittedHeadNotFound(H256),
    #[error(
        "code validation mismatch for code({code_id:?}), local status: {local_status}, remote status: {remote_status}"
    )]
    CodeValidationStatusMismatch {
        code_id: CodeId,
        local_status: bool,
        remote_status: bool,
    },
    #[error("validator set not found for block({0})")]
    ValidatorSetNotFound(H256),

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

pub trait ProcessorExt: Sized + Unpin + Send + Clone + 'static {
    /// Process block events and return the result.
    fn process_block_events(
        &mut self,
        block: H256,
        events: Vec<BlockRequestEvent>,
    ) -> impl Future<Output = Result<BlockProcessingResult>> + Send;
    fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<bool>;
}

impl ProcessorExt for Processor {
    async fn process_block_events(
        &mut self,
        block: H256,
        events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        self.process_block_events(block, events)
            .await
            .map_err(Into::into)
    }

    fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<bool> {
        self.process_upload_code(code_and_id).map_err(Into::into)
    }
}
