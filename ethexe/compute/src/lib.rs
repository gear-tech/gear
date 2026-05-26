// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Compute
//!
//! Three pipelines that turn on-chain data and Malachite-finalised
//! blocks into executed state on the ethexe node: code validation,
//! Ethereum-block preparation, and Malachite-block (MB) execution.
//! Each pipeline is owned by an independent sub-service inside
//! [`ComputeService`]; the outer [`crate::ComputeService`] composes
//! them and exposes progress as a `futures::Stream` of [`ComputeEvent`]s.
//!
//! - `codes` — validates and instruments a WASM code blob and marks its
//!   validity in the database. Emits [`ComputeEvent::CodeProcessed`].
//! - `prepare` — brings a synced Ethereum block (and any not-yet-prepared
//!   ancestors) into a state where its events can be folded into MB
//!   execution, requesting missing code blobs from the caller along
//!   the way. Emits [`ComputeEvent::RequestLoadCodes`] and
//!   [`ComputeEvent::BlockPrepared`].
//! - `mb_compute` — executes a finalised Malachite block (computing
//!   any missing ancestor MBs first) by walking its `Transactions`
//!   list through `ethexe-processor`. Emits [`ComputeEvent::MbComputed`].
//!
//! ## Role in the stack
//!
//! - `ethexe-processor` is the backend. Compute is generic over the
//!   [`ProcessorExt`] trait defined here and has a direct impl for
//!   [`Processor`]; the only other impl in the tree is a test mock
//!   (`tests::MockProcessor`).
//! - `ethexe-blob-loader` is **not** a direct dependency. When `prepare`
//!   discovers codes with unknown validation status it yields
//!   [`ComputeEvent::RequestLoadCodes`] upstream; the service layer
//!   calls the blob loader and feeds the loaded bytes back through
//!   [`ComputeService::process_code`].
//! - `ethexe-db` is the only place compute reads from and writes to.
//! - `ethexe-service` polls the `futures::Stream` and routes each
//!   event onward (consensus, network, blob-loader).
//!
//! ## Entry points
//!
//! - [`ComputeService::process_code`] — queue a code blob for validation +
//!   instrumentation + DB persistence.
//! - [`ComputeService::prepare_block`] — queue a synced Eth block for
//!   preparation (walks ancestors, requests codes).
//! - [`ComputeService::compute_mb`] — queue a finalised MB for execution
//!   (walks uncomputed ancestor MBs first).
//! - `<ComputeService as Stream>::poll_next` — drive all sub-services
//!   and yield the next [`ComputeEvent`].
//!
//! ## Code processing pipeline (`codes`)
//!
//! For every code submitted through [`ComputeService::process_code`] the
//! stream eventually yields exactly one [`ComputeEvent::CodeProcessed`]
//! (carrying the same `CodeId`) or a [`ComputeError`]. Multiple codes
//! submitted at once can be processed concurrently.
//!
//! ## Block preparation pipeline (`prepare`)
//!
//! For every block hash submitted through [`ComputeService::prepare_block`]
//! the stream eventually yields exactly one [`ComputeEvent::BlockPrepared`]
//! or a [`ComputeError`]. Before the block-prepared event the stream may
//! emit one or more [`ComputeEvent::RequestLoadCodes`] if the block — or
//! any of its still-unprepared ancestors — references codes whose validity
//! has not yet been established.
//!
//! ## MB computation pipeline (`mb_compute`)
//!
//! For every MB hash submitted through [`ComputeService::compute_mb`] the
//! stream yields one [`ComputeEvent::MbComputed`] once the MB and any
//! uncomputed ancestor MBs have been executed. Compute walks the parent
//! chain via [`ethexe_common::db::CompactMb::parent`] until it reaches
//! a computed ancestor (or genesis), then runs the executor over the
//! [`ethexe_common::malachite::Transactions`] payload of each. Per-step gas
//! budget is carried inside each `Transaction::ProcessQueues` payload
//! (see [`ethexe_common::malachite::ProcessQueuesLimits`]).
//!
//! ## Canonical event quarantine
//!
//! Ethereum events do not become visible to the runtime on the block
//! they arrive in. When the executor processes an
//! `AdvanceTillEthereumBlock` transaction inside an MB it fetches the
//! events from blocks already past the canonical-quarantine window
//! (`MalachiteConfig::canonical_quarantine` in `ethexe-malachite` —
//! enforced inside `ethexe-processor`'s `process_programs`).
//!
//! ## When modifying this crate
//!
//! - A code result must reach the `prepare` sub-service before the
//!   corresponding `CodeProcessed` is emitted upstream, otherwise a
//!   block waiting on that code will stall for an extra poll.
//! - `compute_mb` must only be called once the malachite service has
//!   recorded the matching `CompactMb` + transactions blob. The
//!   service layer enforces this by gating event emission inside
//!   `MalachiteService::receive_new_chain_head` (in `ethexe-malachite`).

use ethexe_common::{CodeAndIdUnchecked, injected::Promise};
use ethexe_processor::{
    BoundPromiseSink, ExecutableData, ProcessedCodeInfo, Processor, ProcessorError,
};
use ethexe_runtime_common::FinalizedBlockTransitions;
use gprimitives::{CodeId, H256};
use std::collections::HashSet;

pub use compute::{ComputeSubService, prepare_executable_for_mb};
pub use service::ComputeService;

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
    #[from(skip)]
    MbComputed(H256),
    Promise(Promise, H256),
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
    #[error("codes queue not found for computed block({0})")]
    CodesQueueNotFound(H256),
    #[error("last committed batch not found for computed block({0})")]
    LastCommittedBatchNotFound(H256),
    #[error(
        "Received validators commitment for an earlier era {commitment_era_index}, previous was {previous_commitment_era_index}"
    )]
    ValidatorsCommittedForEarlierEra {
        previous_commitment_era_index: u64,
        commitment_era_index: u64,
    },
    #[error("MB payload {payload_hash} not found for mb {mb_hash}")]
    MbPayloadNotFound { mb_hash: H256, payload_hash: H256 },
    #[error("MB {0} CompactMb is missing")]
    MbCompactNotFound(H256),
    #[error("parent MB {0} marked computed but program_states row missing")]
    ParentMbStatesMissing(H256),
    #[error("parent MB {0} marked computed but schedule row missing")]
    ParentMbScheduleMissing(H256),
    #[error("block events row missing for advance-chain block({0})")]
    AdvanceBlockEventsMissing(H256),
    #[error("anchor Eth block header missing for {0}")]
    AnchorBlockHeaderMissing(H256),
    #[error("AdvanceTillEthereumBlock walk hit a missing parent header at {hash}")]
    AdvanceMissingHeader { hash: H256 },
    #[error(
        "AdvanceTillEthereumBlock walk from {target} to {last_advanced} exceeded the safety cap"
    )]
    AdvanceWalkTooDeep { target: H256, last_advanced: H256 },

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

pub trait ProcessorExt: Sized + Unpin + Send + Clone + 'static {
    /// Run the processor's pipeline against `executable`.
    fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_sink: Option<BoundPromiseSink>,
    ) -> impl Future<Output = Result<FinalizedBlockTransitions>> + Send;
    fn process_code(
        &mut self,
        code_and_id: CodeAndIdUnchecked,
    ) -> impl Future<Output = Result<ProcessedCodeInfo>> + Send;
}

impl ProcessorExt for Processor {
    async fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_sink: Option<BoundPromiseSink>,
    ) -> Result<FinalizedBlockTransitions> {
        self.process_programs(executable, promise_sink)
            .await
            .map_err(Into::into)
    }

    async fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo> {
        self.process_code(code_and_id).await.map_err(Into::into)
    }
}
