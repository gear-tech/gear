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

//! # Ethexe Compute
//!
//! Orchestrates the three pipelines that turn on-chain data into executed
//! state for the ethexe node: code validation, block preparation, and
//! announce computation. The crate wraps `ethexe-processor` and exposes its
//! progress as a `futures::Stream` of [`ComputeEvent`]s: the outer service
//! submits work through a few input methods, then polls the stream and
//! handles each event that comes out.
//!
//! [`ComputeService`] composes three independent sub-services. Each does
//! one thing and emits one family of events:
//!
//! - `codes` — validates and instruments a WASM code blob and marks its
//!   validity in the database. Emits [`ComputeEvent::CodeProcessed`].
//! - `prepare` — brings a synced block (and any not-yet-prepared ancestors)
//!   into a state where it can be executed, requesting missing code blobs
//!   from the caller along the way. Emits [`ComputeEvent::RequestLoadCodes`]
//!   and [`ComputeEvent::BlockPrepared`].
//! - `compute` — executes an announce (computing any missing ancestor
//!   announces first), optionally streaming promises for it. Emits
//!   [`ComputeEvent::Promise`] and [`ComputeEvent::AnnounceComputed`].
//!
//! ## Role in the stack and relation to other crates
//!
//! - `ethexe-processor` is the backend. Compute is generic over the
//!   [`ProcessorExt`] trait defined here and has a blanket impl for
//!   [`Processor`]; the only other impl in the tree is a test mock
//!   (`tests::MockProcessor`) that lets the sub-service tests run without
//!   any real WASM execution.
//! - `ethexe-blob-loader` is **not** a direct dependency. When `prepare`
//!   discovers codes with unknown validation status, it yields
//!   [`ComputeEvent::RequestLoadCodes`] upstream; the service layer is
//!   responsible for calling the blob loader, and then feeds the loaded
//!   bytes back into compute via [`ComputeService::process_code`]. That
//!   way compute itself never has to make network calls.
//! - `ethexe-db` is the only place compute reads from and writes to.
//! - `ethexe-service` is the sole consumer: it polls the `futures::Stream`
//!   produced by [`ComputeService`] inside the main `tokio::select!` loop
//!   and routes each [`ComputeEvent`] variant to the rest of the node
//!   (consensus, network, blob-loader).
//!
//! ## Entry points
//!
//! | Method                                       | Effect                                                                                  |
//! |----------------------------------------------|-----------------------------------------------------------------------------------------|
//! | [`ComputeService::process_code`]             | Queue a code blob for validation + instrumentation + DB persistence.                    |
//! | [`ComputeService::prepare_block`]            | Queue a synced block for preparation (walks ancestors, emits code requests).            |
//! | [`ComputeService::compute_announce`]         | Queue an announce for execution with a [`PromisePolicy`](ethexe_common::PromisePolicy). |
//! | `<ComputeService as Stream>::poll_next`      | Drive all three sub-services and yield the next [`ComputeEvent`].                       |
//!
//! ## Code processing pipeline (`codes` sub-service)
//!
//! For every code submitted through [`ComputeService::process_code`] the
//! stream eventually yields exactly one [`ComputeEvent::CodeProcessed`]
//! (carrying the same `CodeId`) or a [`ComputeError`]. This holds both
//! for fresh codes and for codes that had already been validated in a
//! previous run, so the caller does not have to de-duplicate.
//!
//! Multiple codes submitted at once can be processed concurrently.
//!
//! ## Block preparation pipeline (`prepare` sub-service)
//!
//! For every block hash submitted through [`ComputeService::prepare_block`]
//! the stream eventually yields exactly one [`ComputeEvent::BlockPrepared`]
//! for that hash or a [`ComputeError`]. Before the block-prepared event,
//! the stream may emit one or more [`ComputeEvent::RequestLoadCodes`] if
//! the block — or any of its still-unprepared ancestors — references codes
//! whose validity has not yet been established. The caller must fetch
//! those codes (out of scope for this crate) and feed them back in through
//! [`ComputeService::process_code`]; preparation resumes automatically as
//! the missing codes arrive.
//!
//! Error conditions visible to the caller:
//!
//! - [`ComputeError::BlockNotSynced`] — the observer has not stored the
//!   block yet.
//! - [`ComputeError::ValidatorsCommittedForEarlierEra`] — the block
//!   attempts to regress the validators era index relative to its parent.
//!
//! ## Announce computation pipeline (`compute` sub-service)
//!
//! For every announce submitted through [`ComputeService::compute_announce`]
//! with a [`PromisePolicy`](ethexe_common::PromisePolicy), the stream
//! eventually yields exactly one [`ComputeEvent::AnnounceComputed`] for
//! that announce or a [`ComputeError`]. If the caller passed
//! [`PromisePolicy::Enabled`](ethexe_common::PromisePolicy), zero or more
//! [`ComputeEvent::Promise`] events for the same announce are yielded
//! first. Every `Promise` for a given announce is yielded strictly before
//! the `AnnounceComputed` of that announce — `AnnounceComputed` is the
//! "all promises for this announce have been delivered" marker.
//!
//! Computation is sequential: at most one announce is executed at a time.
//! If the announce's parent (or any further ancestor) has not been
//! computed yet, missing ancestors are computed first, in order.
//! Ancestors are always computed without promise collection regardless of
//! the requested policy — promises describe the user-visible result of
//! the target announce only.
//!
//! The target block must already be prepared; otherwise the computation
//! fails with [`ComputeError::BlockNotPrepared`].
//!
//! Actual WASM execution is delegated to [`ProcessorExt::process_programs`].
//!
//! ## Canonical event quarantine
//!
//! Ethereum events do not become visible to the runtime on the block they
//! arrive in. When building the execution input for a block, compute
//! instead takes the events from an ancestor that is
//! [`ComputeConfig::canonical_quarantine`](ComputeConfig) blocks older
//! (the genesis block is the floor). In production this delay must equal
//! [`CANONICAL_QUARANTINE`](ethexe_common::gear::CANONICAL_QUARANTINE);
//! [`ComputeConfig::without_quarantine`] is strictly for tests.
//!
//! ## Event flow summary
//!
//! | [`ComputeEvent`]          | Fired by | Expected consumer                                     |
//! |---------------------------|----------|-------------------------------------------------------|
//! | `CodeProcessed(code_id)`  | `codes`  | Informational.                                        |
//! | `RequestLoadCodes(set)`   | `prepare`| Handed to `ethexe-blob-loader` to fetch code blobs.   |
//! | `BlockPrepared(hash)`     | `prepare`| Handed to `ethexe-consensus`.                         |
//! | `AnnounceComputed(hash)`  | `compute`| Handed to `ethexe-consensus`.                         |
//! | `Promise(p, ah)`          | `compute`| Handed to `ethexe-consensus` for signing.             |
//!
//! ## When modifying this crate
//!
//! - A code result must reach the `prepare` sub-service before the
//!   corresponding `CodeProcessed` is emitted upstream, otherwise a block
//!   waiting on that code will stall for an extra poll.
//! - An announce must only be computed after its block has been prepared.
//! - For announce execution, canonical events must always be read via
//!   [`find_canonical_events_post_quarantine`], never directly via
//!   `db.block_events(...)` from the announce's own block. Taking the raw
//!   events would skip the quarantine and produce non-deterministic state
//!   across nodes that disagree on a recent reorg.
//! - For any single announce, `AnnounceComputed` must be the last event
//!   emitted; every `Promise` that belongs to it comes strictly before.

pub use compute::{
    ComputeConfig, ComputeSubService,
    utils::{find_canonical_events_post_quarantine, prepare_executable_for_announce},
};
use ethexe_common::{Announce, CodeAndIdUnchecked, HashOf, injected::Promise};
use ethexe_processor::{ExecutableData, ProcessedCodeInfo, Processor, ProcessorError};
use ethexe_runtime_common::FinalizedBlockTransitions;
use gprimitives::{CodeId, H256};
pub use service::ComputeService;
use std::collections::HashSet;
use tokio::sync::mpsc;

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
    Promise(Promise, HashOf<Announce>),
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
    CommittedEraNotFound(H256),
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
    fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> impl Future<Output = Result<FinalizedBlockTransitions>> + Send;
    fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo>;
}

impl ProcessorExt for Processor {
    async fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<FinalizedBlockTransitions> {
        self.process_programs(executable, promise_out_tx)
            .await
            .map_err(Into::into)
    }

    fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo> {
        self.process_code(code_and_id).map_err(Into::into)
    }
}
