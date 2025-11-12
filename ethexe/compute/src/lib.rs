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

pub use compute::ComputeConfig;
use ethexe_common::{Announce, CodeAndIdUnchecked, HashOf, events::BlockRequestEvent};
use ethexe_processor::{BlockProcessingResult, Processor, ProcessorError};
use gprimitives::{CodeId, H256};
pub use service::ComputeService;
use std::collections::HashSet;

mod codes;
mod compute;
mod prepare;
mod service;

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
    AnnounceComputed(HashOf<Announce>),
}

#[derive(thiserror::Error, Debug)]
pub enum ComputeError {
    #[error("block({0}) is not synced")]
    BlockNotSynced(H256),
    #[error("block({0}) is not prepared")]
    BlockNotPrepared(H256),
    #[error("not found events for block({0})")]
    BlockEventsNotFound(H256),
    #[error("block header not found for synced block({0})")]
    BlockHeaderNotFound(H256),
    #[error("process code join error")]
    CodeProcessJoin(#[from] tokio::task::JoinError),
    #[error("codes queue not found for computed block({0})")]
    CodesQueueNotFound(H256),
    #[error("last committed batch not found for computed block({0})")]
    LastCommittedBatchNotFound(H256),
    #[error("last committed head not found for computed block({0})")]
    LastCommittedHeadNotFound(H256),
    #[error("Announce {0:?} not found in db")]
    AnnounceNotFound(HashOf<Announce>),
    #[error("Announces for prepared block {0:?} not found in db")]
    PreparedBlockAnnouncesSetMissing(H256),
    #[error("Latest data not found")]
    LatestDataNotFound,

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

pub trait ProcessorExt: Sized + Unpin + Send + Clone + 'static {
    /// Process block events and return the result.
    fn process_announce(
        &mut self,
        announce: Announce,
        events: Vec<BlockRequestEvent>,
    ) -> impl Future<Output = Result<BlockProcessingResult>> + Send;
    fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<bool>;
}

impl ProcessorExt for Processor {
    async fn process_announce(
        &mut self,
        announce: Announce,
        events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        self.process_announce(announce, events)
            .await
            .map_err(Into::into)
    }

    fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<bool> {
        self.process_upload_code(code_and_id).map_err(Into::into)
    }
}
