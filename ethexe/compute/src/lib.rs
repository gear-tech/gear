// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Compute
//!
//! Orchestrates turning on-chain data and Malachite-finalised blocks into executed state on
//! an ethexe node through three independent pipelines: code validation, Ethereum-block
//! preparation, and Malachite-block (MB) execution.
//!
//! ## Responsibilities
//!
//! Each pipeline runs as an independent sub-service composed inside [`ComputeService`]:
//!
//! - **`codes`** — validates and instruments a WASM code blob and marks its validity in the
//!   database. Emits [`ComputeEvent::CodeProcessed`].
//! - **`prepare`** — brings a synced Ethereum block (and any not-yet-prepared ancestors) into
//!   a state where its events can be folded into MB execution, requesting missing code blobs
//!   from the caller along the way. Emits [`ComputeEvent::RequestLoadCodes`] and
//!   [`ComputeEvent::BlockPrepared`].
//! - **`mb_compute`** — executes a finalised Malachite block (computing any missing ancestor
//!   MBs first) by walking its `Transactions` list through `ethexe-processor`. Emits
//!   [`ComputeEvent::MbComputed`].
//!
//! ## Role in the stack
//!
//! ```text
//! ethexe-observer     (decodes Ethereum blocks / events)
//!        |
//! ethexe-service ──→ ComputeService::prepare_block(block)
//!                          |
//!                    prepare sub-service
//!                          |  RequestLoadCodes → ethexe-blob-loader → process_code()
//!                          ↓
//!                    codes sub-service  (validates, instruments via ProcessorExt)
//!                          |
//!                    ethexe-service ──→ ComputeService::compute_mb(mb_hash, policy)
//!                          |
//!                    mb_compute sub-service
//!                          |  ProcessorExt::process_programs → ethexe-processor
//!                          ↓
//!                    ComputeEvent::MbComputed ──→ ethexe-service
//! ```
//!
//! - [`ProcessorExt`] abstracts the execution backend; the production impl delegates to
//!   `ethexe-processor`'s [`Processor`]; tests inject a `MockProcessor`.
//! - `ethexe-blob-loader` is **not** a direct dependency. When `prepare` discovers codes of
//!   unknown validity it yields [`ComputeEvent::RequestLoadCodes`]; the service layer calls
//!   the blob loader and feeds bytes back through [`ComputeService::process_code`].
//! - `ethexe-db` is the only storage layer compute reads from and writes to.
//! - `ethexe-service` polls the [`futures::Stream`](ComputeService) implementation and routes
//!   each [`ComputeEvent`] onward (to consensus, network, or the blob loader).
//!
//! ## Entry points / Public API
//!
//! | Method | Description |
//! |--------|-------------|
//! | [`ComputeService::new`] | Default constructor; uses `ConsensusDriven` promise mode. |
//! | [`ComputeService::with_promise_mode`] | Constructor used by `ethexe-service`; allows `AlwaysEmit` mode for RPC nodes replaying the chain. |
//! | [`ComputeService::process_code`] | Queue a code blob for validation, instrumentation, and DB persistence. |
//! | [`ComputeService::prepare_block`] | Queue a synced Eth block for preparation (walks ancestors, requests codes). |
//! | [`ComputeService::compute_mb`] | Queue a finalised MB for execution (walks uncomputed ancestors first). |
//! | `Stream::poll_next` on [`ComputeService`] | Drive all sub-services and yield the next [`ComputeEvent`]. |
//!
//! Re-exported from sub-modules: [`ComputeSubService`], [`prepare_executable_for_mb`].
//!
//! ## Key types
//!
//! - [`ComputeService`] — top-level composed service; generic over `P: ProcessorExt`;
//!   implements `futures::Stream<Item = std::result::Result<ComputeEvent, ComputeError>>`.
//! - [`ComputeEvent`] — unit of stream output: `RequestLoadCodes`, `CodeProcessed`,
//!   `BlockPrepared`, `MbComputed`, `Promise`.
//! - [`ComputeError`] — exhaustive pipeline error set; most variants signal missing or
//!   inconsistent DB rows; `Processor` variant is transparent over `ProcessorError`.
//! - [`ProcessorExt`] — backend abstraction: `process_programs` and `process_code`.
//!   Bounds: `Sized + Unpin + Send + Clone + 'static`.
//! - [`ComputeSubService`] — the MB-execution sub-service (re-exported from `compute`).
//!
//! ## Invariants
//!
//! 1. **Code pipeline**: for every code queued via [`ComputeService::process_code`] the
//!    stream eventually yields exactly one [`ComputeEvent::CodeProcessed`] (same `CodeId`)
//!    or a [`ComputeError`]. Concurrent codes are processed concurrently.
//!
//! 2. **Block preparation pipeline**: for every block hash queued via
//!    [`ComputeService::prepare_block`] the stream yields exactly one
//!    [`ComputeEvent::BlockPrepared`] or a [`ComputeError`]. One or more
//!    [`ComputeEvent::RequestLoadCodes`] may precede it when block ancestors reference
//!    codes of unknown validity.
//!
//! 3. **MB computation pipeline**: for every MB hash queued via
//!    [`ComputeService::compute_mb`] the stream yields one [`ComputeEvent::MbComputed`]
//!    once the MB and all uncomputed ancestor MBs have been executed. The parent chain is
//!    walked via `CompactMb::parent` to a computed ancestor or genesis.
//!
//! 4. **Canonical event quarantine**: Ethereum events do not become visible to the runtime
//!    on the block they arrive in; the quarantine window is controlled by
//!    `MalachiteConfig::canonical_quarantine` (in `ethexe-malachite`) and is applied when
//!    the MB producer selects which Ethereum block to advance to; `process_programs` only
//!    executes the pre-quarantined payload.
//!
//! 5. **Ordering**: a code result must reach the `prepare` sub-service before the
//!    corresponding `CodeProcessed` is emitted upstream, otherwise a block waiting on that
//!    code stalls for an extra poll.
//!
//! 6. **Caller contract**: [`ComputeService::compute_mb`] must only be called after the
//!    malachite service has recorded the matching `CompactMb` and transactions blob in the
//!    database.

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
