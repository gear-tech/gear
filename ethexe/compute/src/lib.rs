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

use ethexe_common::{Announce, CodeAndIdUnchecked, HashOf, injected::Promise};
use ethexe_processor::{ExecutableData, ProcessedCodeInfo, Processor, ProcessorError};
use ethexe_runtime_common::FinalizedBlockTransitions;
use gprimitives::{CodeId, H256};
use std::collections::HashSet;

pub use compute::{ComputeConfig, ComputeSubService, prepare_executable_for_announce};
pub use service::{ComputeService, builder::Builder as ComputeServiceBuilder};

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

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap, derive_more::From)]
pub enum ComputeEvent {
    RequestLoadCodes(HashSet<CodeId>),
    CodeProcessed(CodeId),
    BlockPrepared(H256),
    AnnounceComputed(HashOf<Announce>),
    Promise(Promise),
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
    #[error("block validators committed for era not found for block({0})")]
    BlockValidatorsCommittedForEraNotFound(H256),
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
    #[error(
        "Received validators commitment for an earlier era {commitment_era_index}, previous was {previous_commitment_era_index}"
    )]
    ValidatorsCommittedForEarlierEra {
        previous_commitment_era_index: u64,
        commitment_era_index: u64,
    },
    #[error("Program states not found for computed Announce {0:?}")]
    ProgramStatesNotFound(HashOf<Announce>),
    #[error("Schedule not found for computed Announce {0:?}")]
    ScheduleNotFound(HashOf<Announce>),
    #[error("Promise sender dropped")]
    PromiseSenderDropped,

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

pub trait ProcessorExt: Sized + Unpin + Send + Clone + 'static {
    /// Process block events and return the result.
    fn process_announce(
        &mut self,
        executable: ExecutableData,
    ) -> impl Future<Output = Result<FinalizedBlockTransitions>> + Send;
    fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked)
    -> Result<ProcessedCodeInfo>;
}

impl ProcessorExt for Processor {
    async fn process_announce(
        &mut self,
        executable: ExecutableData,
    ) -> Result<FinalizedBlockTransitions> {
        self.process_programs(executable).await.map_err(Into::into)
    }

    fn process_upload_code(
        &mut self,
        code_and_id: CodeAndIdUnchecked,
    ) -> Result<ProcessedCodeInfo> {
        self.process_code(code_and_id).map_err(Into::into)
    }
}
