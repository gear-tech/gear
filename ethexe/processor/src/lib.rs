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

//! # Ethexe Processor
//!
//! Low-level execution engine that runs Gear programs inside the ethexe node.
//! The crate drives a pre-compiled [ethexe runtime](ethexe_runtime) WASM artifact
//! in [wasmtime], linking host functions that expose the database, lazy pages,
//! sandboxed nested WASM, promise publishing, allocation and logging to the
//! runtime. On top of that the crate implements all the plumbing needed to
//! execute an ethexe block(announce):
//! - routing ethereum block events [`BlockRequestEvent`] into program state mutations,
//! - appending injected transactions [`InjectedTransaction`] to program queues,
//! - running scheduled tasks,
//! - draining program message queues through a
//!   parallel chunked executor until gas or other limits are exhausted.
//!
//! ## Role in the stack and relation to other crates
//!
//! `ethexe-processor` is the bottom of the execution stack. It is consumed by:
//!
//! - `ethexe-compute` — calls [`Processor::process_programs`] and
//!   [`Processor::process_code`] through its `ProcessorExt` trait (the trait
//!   is defined in `ethexe-compute`, together with a blanket impl for
//!   [`Processor`]). Compute is what the service layer talks to — the
//!   processor itself is never polled as a stream and emits no events.
//! - `ethexe-rpc` — uses [`OverlaidProcessor`] (obtained via
//!   [`Processor::overlaid`]) to simulate message execution against a
//!   overlaid database for read-only reply queries.
//! - `ethexe-service` — constructs the `Processor` instance at startup and
//!   hands it to `ComputeService`.
//!
//! ## Entry points
//!
//! The public API is intentionally small:
//!
//! | Method                                    | Purpose                                                                 |
//! |-------------------------------------------|-------------------------------------------------------------------------|
//! | [`Processor::process_code`]               | Validate + instrument a WASM blob. Synchronous, pure w.r.t. the DB.     |
//! | [`Processor::process_programs`]           | Execute an ethexe block: events → tasks → queues. Main async workflow.  |
//! | [`Processor::overlaid`]                   | Wrap `self` into an [`OverlaidProcessor`] with a COW view of the DB.    |
//! | [`OverlaidProcessor::execute_for_reply`]  | Simulate a single incoming message and return the reply.                |
//!
//! Everything else in the crate is an internal detail of those four entry
//! points.
//!
//! ## `process_programs` pipeline
//!
//! Given an [`ExecutableData`] (block header, program states, schedule,
//! injected transactions, block request events and optional gas allowance),
//! [`Processor::process_programs`] runs three sequential stages over an
//! `InBlockTransitions` accumulator:
//!
//! 1. **Handle injected transactions and block events** (`handling::events`).
//!    User-injected transactions are appended to program injected queues;
//!    router and mirror events are routed to the corresponding state
//!    mutations (program creation, balance top-up, message queueing, value
//!    claims, …). See `handling::ProcessingHandler`.
//! 2. **Process scheduled tasks** ([`ScheduleHandler`]). Tasks that are due at
//!    the current block height are popped from the schedule and executed
//!    (e.g. mailbox expiry cleanup, reservation removal).
//! 3. **Process queues** (`handling::run`). A `CommonRunContext` drains the
//!    injected queue first, then — if limits allow — the canonical queue,
//!    both through the chunked parallel executor described below. Promises
//!    are collected only during the injected pass; before starting the
//!    canonical pass `CommonRunContext::disable_promises` drops the
//!    `promise_out_tx` sender, so any code that introduces new promise
//!    emission points must make sure they are reached from the injected
//!    queue.
//!
//! The final [`ethexe_runtime_common::FinalizedBlockTransitions`] is returned
//! to the caller.
//!
//! ## Chunked parallel execution
//!
//! Inside `handling::run` the non-empty program queues for a given queue type
//! are partitioned into `ProcessorConfig::chunk_size` chunks (default
//! [`DEFAULT_CHUNK_SIZE`]) by `chunks_splitting`. Within a chunk, every
//! program runs in parallel: each one is submitted as a separate task to the
//! `handling::thread_pool` worker pool, gets its own wasmtime `Store`
//! (`host::InstanceWrapper`), and receives a copy of the chunk's gas
//! allowance — see the `spawn_chunk_execution` comment on why they do not
//! share a single counter. After all programs in a chunk finish, the executor
//! collects their journals, applies the resulting side effects (outgoing
//! dispatches, mailbox entries, value claims, promise emissions, new
//! scheduled tasks), and charges the block gas allowance counter by the
//! **maximum** gas spent across the chunk (not the sum — programs inside a
//! chunk run simultaneously, so the wall-clock bottleneck is the slowest
//! one). The loop repeats until:
//!
//! - all program queues are empty, or
//! - the gas allowance is exhausted, or
//! - one of the configured soft limits kicks in (outgoing messages count,
//!   payload bytes, call replies, program modifications).
//!
//! ## Runtime hosting (`host` module)
//!
//! `host::InstanceCreator` owns the shared wasmtime `Engine` and a
//! pre-compiled module of [`ethexe_runtime::WASM_BINARY_BLOATY`]. It registers
//! host functions — allocator, database, lazy pages, logging, sandbox, promise
//! — in a single `Linker`. Each program execution obtains an
//! `host::InstanceWrapper`, which calls the runtime's two exports:
//!
//! - `instrument_code(code_ptr, code_len) -> i64` — runs the ethexe runtime
//!   validation + instrumentation pipeline on a raw WASM blob (used by
//!   [`Processor::process_code`]).
//! - `run(ctx_ptr, ctx_len) -> i64` — executes one program's queue and
//!   returns executions results (journals) and gas spent, see for more
//!   `host::InstanceWrapper::run`.
//!
//! Per-execution state (the database handle, the current program state hash,
//! the optional promise sender, lazy-pages caches) is threaded into host
//! functions through the `host::threads::PARAMS` thread-local. State hashes
//! are reported back from the runtime via the
//! `ext_update_state_hash_version_1` host function and captured there; the
//! host-side executor reads the final value from the thread-local after the
//! `run` export returns.
//!
//! ### Lazy pages
//!
//! Program memory is not materialized up front. Pages are protected after
//! instance setup and loaded from the database on the first SIGSEGV(on linux),
//! via the [`gear_lazy_pages`] integration. See `host::threads::EthexeHostLazyPages`.
//!
//! ## Overlay execution
//!
//! [`OverlaidProcessor`] wraps a [`Processor`] whose database is swapped for a
//! overlaid database [`Database::overlaid`]. Mutations are kept in memory
//! and discarded when the overlay is dropped, so the underlying state is never
//! touched. [`OverlaidProcessor::execute_for_reply`] synthesizes a single
//! [`MessageQueueingRequestedEvent`] into the target program's canonical queue
//! and runs inside an `handling::overlaid::OverlaidRunContext` which:
//!
//! - trims the target program's canonical queue to only the synthetic
//!   dispatch in the constructor
//!   (`OverlaidRunContext::new`), so the simulation starts from a clean slate
//!   for the target;
//! - when any other program is about to be scheduled and its queue has not
//!   been touched yet, clears the queue and skips the scheduled run
//!   (`OverlaidRunContext::check_task_no_run`) — non-target programs only
//!   ever execute messages that originate from the current simulation;
//! - when a journal emits `SendDispatch` to a receiver, clears that
//!   receiver's queue first so it will process only the cascading message
//!   (`OverlaidRunContext::nullify_receivers_queues`);
//! - watches journals for a reply referencing the synthetic
//!   [`MessageId::zero()`] and, as soon as it sees one, short-circuits the
//!   executor without performing any further nullifications
//!   (`OverlaidRunContext::nullify_or_break_early`).
//!
//! ## Determinism notes
//!
//! - The chunking algorithm is a pure function of the program → queue-size map
//!   and `chunk_size`, so every node executing the same block partitions the
//!   work identically.
//! - The host-side gas counter increments by max spending in chunk gas; WASM-side
//!   state hashing runs inside the WASM runtime and does not depend on chunk layout.
//! - WASM traps (out-of-bounds memory, `unreachable`, wasmtime errors) and
//!   host-function panics that go through the `sp_wasm_interface`
//!   `register_panic_error_message` hook are surfaced as
//!   [`InstanceError::Wasmtime`] and propagated out of
//!   [`Processor::process_programs`]. Raw Rust panics inside a chunk worker
//!   are caught by `handling::thread_pool` with `catch_unwind` and re-raised
//!   on the caller via `resume_unwind` — they unwind the async task, they do
//!   not become an `Err` variant.
//!
//! ## Configuration
//!
//! [`ProcessorConfig`] currently exposes a single knob, `chunk_size`, which
//! controls the number of programs executed in parallel per pass. The default
//! is [`DEFAULT_CHUNK_SIZE`] (16).
//!
//! ## When modifying this crate
//!
//! - Processor must be deterministic.
//! - Changing Processor logic may cause collisions in already deployed ethexe networks,
//!   so be careful when modifying the processing pipeline,
//!   and always check backwards compatibility with deployed networks.
//! - Processor is designed to write only in CAS, it must NEVER modify key-value storage from Database.

use core::num::NonZero;
use ethexe_common::{
    CodeAndIdUnchecked, ProgramStates, Schedule, SimpleBlockData,
    ecdsa::VerifiedData,
    events::{BlockRequestEvent, MirrorRequestEvent, mirror::MessageQueueingRequestedEvent},
    injected::{InjectedTransaction, Promise},
};
use ethexe_db::Database;
use ethexe_runtime_common::{
    FinalizedBlockTransitions, InBlockTransitions, ScheduleHandler, TransitionController,
    state::Storage,
};
use gear_core::{
    code::{CodeMetadata, InstrumentationStatus, InstrumentedCode},
    ids::prelude::CodeIdExt,
    rpc::ReplyInfo,
};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use handling::{ProcessingHandler, overlaid::OverlaidRunContext, run::CommonRunContext};
use host::InstanceCreator;
use tokio::sync::mpsc;

pub use host::InstanceError;

mod handling;
mod host;

#[cfg(test)]
mod tests;

// Default amount of programs in one chunk to be processed in parallel.
pub const DEFAULT_CHUNK_SIZE: NonZero<usize> = NonZero::new(16).unwrap();

#[derive(thiserror::Error, Debug)]
pub enum ProcessorError {
    #[error("program {actor_id} was created with unknown or invalid code {code_id}")]
    MissingCode { actor_id: ActorId, code_id: CodeId },

    #[error("code id not found for created program {0}")]
    MissingCodeIdForProgram(ActorId),

    #[error("missing instrumented code for code id {0}")]
    MissingInstrumentedCodeForProgram(CodeId),

    #[error("injected message {0:?} was sent to uninitialized program")]
    InjectedToUninitializedProgram(Box<InjectedTransaction>),

    #[error("calling or instantiating runtime error: {0}")]
    Runtime(#[from] host::InstanceError),

    #[error("anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum ExecuteForReplyError {
    #[error("program {0} isn't yet initialized")]
    ProgramNotInitialized(ActorId),
    #[error("reply wasn't found")]
    ReplyNotFound,
    #[error("not found state hash for program ({0})")]
    ProgramStateHashNotFound(ActorId),
    #[error("not found program state by hash ({0}) in database")]
    ProgramStateNotFound(H256),

    #[error("processor base error: {0}")]
    Processor(#[from] ProcessorError),
}

type Result<T, E = ProcessorError> = std::result::Result<T, E>;

#[derive(Clone, Debug)]
pub struct ProcessorConfig {
    /// Number of programs to be processed in one chunk (in parallel).
    pub chunk_size: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE.get(),
        }
    }
}

#[derive(Clone)]
pub struct Processor {
    config: ProcessorConfig,
    db: Database,
    creator: InstanceCreator,
}

/// TODO: consider avoiding re-instantiations on processing events.
/// Maybe impl `struct EventProcessor`.
impl Processor {
    /// Creates processor with default config.
    pub fn new(db: Database) -> Result<Self> {
        Self::with_config(Default::default(), db)
    }

    pub fn with_config(config: ProcessorConfig, db: Database) -> Result<Self> {
        let creator = InstanceCreator::new(host::runtime())?;
        Ok(Self {
            config,
            db,
            creator,
        })
    }

    pub fn config(&self) -> &ProcessorConfig {
        &self.config
    }

    pub fn overlaid(mut self) -> OverlaidProcessor {
        self.db = unsafe { self.db.overlaid() };

        OverlaidProcessor(self)
    }

    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo> {
        log::debug!("Processing upload code {code_and_id:?}");

        let CodeAndIdUnchecked { code, code_id } = code_and_id;

        if code_id != CodeId::generate(&code) {
            return Ok(ProcessedCodeInfo {
                code_id,
                valid: None,
            });
        }

        let Some((instrumented_code, code_metadata)) =
            self.creator.instantiate()?.instrument(&code)?
        else {
            return Ok(ProcessedCodeInfo {
                code_id,
                valid: None,
            });
        };

        let InstrumentationStatus::Instrumented { .. } = code_metadata.instrumentation_status()
        else {
            panic!("Instrumented code returned, but instrumentation status is not Instrumented");
        };

        Ok(ProcessedCodeInfo {
            code_id,
            valid: Some(ValidCodeInfo {
                code,
                instrumented_code,
                code_metadata,
            }),
        })
    }

    pub async fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<FinalizedBlockTransitions> {
        log::debug!("{executable}");

        let ExecutableData {
            block,
            program_states,
            schedule,
            injected_transactions,
            gas_allowance,
            events,
        } = executable;

        let mut transitions =
            InBlockTransitions::new(block.header.height, program_states, schedule);

        // First step: push injected to queues and handle block events.
        transitions =
            self.handle_injected_and_events(transitions, injected_transactions, events)?;

        // Second step: process scheduled tasks.
        transitions = self.process_tasks(transitions);

        // Third step: process queues until limits are exhausted or all queues are empty.
        if let Some(gas_allowance) = gas_allowance {
            transitions = self
                .process_queues(transitions, block, gas_allowance, promise_out_tx)
                .await?;
        }

        Ok(transitions.finalize())
    }

    fn handle_injected_and_events(
        &mut self,
        transitions: InBlockTransitions,
        injected_transactions: Vec<VerifiedData<InjectedTransaction>>,
        events: Vec<BlockRequestEvent>,
    ) -> Result<InBlockTransitions> {
        let mut handler = ProcessingHandler::new(self.db.clone(), transitions);

        for tx in injected_transactions {
            let source = tx.address().into();
            let tx = tx.into_parts().0;
            handler.handle_injected_transaction(source, tx)?;
        }

        for event in events {
            match event {
                BlockRequestEvent::Router(event) => {
                    handler.handle_router_event(event)?;
                }
                BlockRequestEvent::Mirror { actor_id, event } => {
                    handler.handle_mirror_event(actor_id, event)?;
                }
            }
        }

        Ok(handler.into_transitions())
    }

    async fn process_queues(
        &mut self,
        transitions: InBlockTransitions,
        block: SimpleBlockData,
        gas_allowance: u64,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<InBlockTransitions> {
        CommonRunContext::new(
            self.db.clone(),
            self.creator.clone(),
            transitions,
            gas_allowance,
            self.config.chunk_size,
            block.header,
            promise_out_tx,
        )
        .run()
        .await
    }

    fn process_tasks(&mut self, mut transitions: InBlockTransitions) -> InBlockTransitions {
        let tasks = transitions.take_actual_tasks();
        let block_height = transitions.block_height();

        log::trace!("Running schedule for #{block_height}: tasks are {tasks:?}");

        let mut handler = ScheduleHandler {
            controller: TransitionController {
                storage: &self.db,
                transitions: &mut transitions,
            },
        };

        for task in tasks {
            let _gas = task.process_with(&mut handler);
        }

        transitions
    }
}

#[derive(Clone, Default)]
pub struct ProcessedCodeInfo {
    pub code_id: CodeId,
    pub valid: Option<ValidCodeInfo>,
}

#[derive(Clone)]
pub struct ValidCodeInfo {
    pub code: Vec<u8>,
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
}

#[derive(Debug, derive_more::Display)]
#[display(
    "{block}, programs amount: {}, schedule len: {}, gas_allowance: {gas_allowance:?},
    injected: {injected_transactions:?},
    events: {events:?}",
    program_states.len(), schedule.len(),
)]
pub struct ExecutableData {
    pub block: SimpleBlockData,
    pub program_states: ProgramStates,
    pub schedule: Schedule,
    pub injected_transactions: Vec<VerifiedData<InjectedTransaction>>,
    pub gas_allowance: Option<u64>,
    pub events: Vec<BlockRequestEvent>,
}

#[cfg(test)]
impl Default for ExecutableData {
    fn default() -> Self {
        Self {
            block: SimpleBlockData::default(),
            program_states: ProgramStates::default(),
            schedule: Schedule::default(),
            injected_transactions: vec![],
            gas_allowance: Some(ethexe_common::DEFAULT_BLOCK_GAS_LIMIT),
            events: vec![],
        }
    }
}

#[derive(Debug, derive_more::Display)]
#[display(
    "Execution for reply at {block:?}: block: {block:?}, \
    program_id: {program_id}, source: {source}, payload len: {}, \
    value: {value}, gas_allowance: {gas_allowance}", payload.len()
)]
pub struct ExecutableDataForReply {
    pub block: SimpleBlockData,
    pub program_states: ProgramStates,
    pub source: ActorId,
    pub program_id: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub gas_allowance: u64,
}

#[derive(Clone, derive_more::AsRef, derive_more::AsMut)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    pub async fn execute_for_reply(
        &mut self,
        executable: ExecutableDataForReply,
    ) -> Result<ReplyInfo, ExecuteForReplyError> {
        log::debug!("{executable}");

        let ExecutableDataForReply {
            block,
            program_states,
            source,
            program_id,
            payload,
            value,
            gas_allowance,
        } = executable;

        let state_hash = program_states
            .get(&program_id)
            .ok_or(ExecuteForReplyError::ProgramStateHashNotFound(program_id))?
            .hash;

        let state = self
            .0
            .db
            .program_state(state_hash)
            .ok_or(ExecuteForReplyError::ProgramStateNotFound(state_hash))?;

        if state.requires_init_message() {
            return Err(ExecuteForReplyError::ProgramNotInitialized(program_id));
        }

        let transitions =
            InBlockTransitions::new(block.header.height, program_states, Schedule::default());

        let transitions = self.0.handle_injected_and_events(
            transitions,
            vec![],
            vec![BlockRequestEvent::Mirror {
                actor_id: program_id,
                event: MirrorRequestEvent::MessageQueueingRequested(
                    MessageQueueingRequestedEvent {
                        id: MessageId::zero(),
                        source,
                        payload: payload.clone(),
                        value,
                        call_reply: true,
                    },
                ),
            }],
        )?;

        let transitions = OverlaidRunContext::new(
            self.0.db.clone(),
            program_id,
            transitions,
            gas_allowance,
            self.0.config.chunk_size,
            self.0.creator.clone(),
            block.header,
        )
        .run()
        .await?;

        let res = transitions
            .current_messages()
            .into_iter()
            .find_map(|(_, message)| {
                message.reply_details.and_then(|details| {
                    (details.to_message_id() == MessageId::zero()).then(|| ReplyInfo {
                        payload: message.payload,
                        value: message.value,
                        code: details.to_reply_code(),
                    })
                })
            })
            .ok_or(ExecuteForReplyError::ReplyNotFound)?;

        Ok(res)
    }
}
