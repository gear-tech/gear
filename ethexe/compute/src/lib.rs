// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Compute
//!
//! Orchestrates turning on-chain data and Malachite-finalised blocks into executed state on
//! an ethexe node through three independent pipelines: code validation, Ethereum-block
//! preparation, and Malachite-block (MB) execution.
//!
//! ## Role in the stack
//!
//! `ethexe-service` drives this crate: it calls [`ComputeService::prepare_block`] and
//! [`ComputeService::compute_mb`], polls [`ComputeService`] (a `futures::Stream`), and
//! routes each [`ComputeEvent`] onward to consensus, network, or the blob loader.
//! `ethexe-db` is the only storage layer compute reads from and writes to. Execution is
//! abstracted behind [`ProcessorExt`]; the production impl delegates to `ethexe-processor`'s
//! `Processor`. `ethexe-blob-loader` is not a direct dependency: when preparation discovers
//! codes of unknown validity it yields [`ComputeEvent::RequestLoadCodes`] and the service
//! layer feeds bytes back through [`ComputeService::process_code`].
//!
//! ## Public API
//!
//! | Item | Description |
//! |------|-------------|
//! | [`ComputeService`] | Top-level composed service, generic over `P: ProcessorExt`; a `futures::Stream` of `Result<ComputeEvent, ComputeError>`. |
//! | [`ComputeService::new`] | Default constructor; uses `ConsensusDriven` promise mode. |
//! | [`ComputeService::with_promise_mode`] | Constructor allowing `AlwaysEmit` mode for RPC nodes replaying the chain. |
//! | [`ComputeService::process_code`] | Queue a code blob for validation, instrumentation, and DB persistence. |
//! | [`ComputeService::prepare_block`] | Queue a synced Eth block for preparation (walks ancestors, requests codes). |
//! | [`ComputeService::compute_mb`] | Queue a finalised MB with a `PromisePolicy` for execution (walks uncomputed ancestors first). |
//! | [`ComputeEvent`] | Stream output: `RequestLoadCodes`, `CodeProcessed`, `BlockPrepared`, `MbComputed`, `Promise`. |
//! | [`ComputeError`] | Pipeline error set; the `Processor` variant is transparent over `ProcessorError`. |
//! | [`ProcessorExt`] | Execution backend abstraction: `process_programs` and `process_code`. |
//! | [`ComputeSubService`], [`prepare_executable_for_mb`] | The MB-execution sub-service and its executable builder. |
//!
//! ## Caller guarantees
//!
//! - For every code queued via [`ComputeService::process_code`] the stream eventually yields
//!   exactly one [`ComputeEvent::CodeProcessed`] (same `CodeId`) or a [`ComputeError`].
//! - For every block queued via [`ComputeService::prepare_block`] the stream yields exactly
//!   one [`ComputeEvent::BlockPrepared`] or a [`ComputeError`], optionally preceded by one or
//!   more [`ComputeEvent::RequestLoadCodes`].
//! - For every MB queued via [`ComputeService::compute_mb`] the stream yields one
//!   [`ComputeEvent::MbComputed`] once the MB and all uncomputed ancestor MBs are executed.
//!
//! ## Caller contract
//!
//! [`ComputeService::compute_mb`] must only be called after the malachite service has recorded
//! the matching `CompactMb` and transactions blob in the database.

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
