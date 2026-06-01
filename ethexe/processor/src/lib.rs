// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Processor
//!
//! Low-level execution engine that runs Gear programs inside the ethexe node. It
//! embeds a pre-compiled `ethexe_runtime` WASM artifact and runs it in `wasmtime`.
//!
//! ## Role in the stack
//!
//! `ethexe-compute` (via its `ProcessorExt` trait) drives this crate to validate
//! code and execute blocks; `ethexe-rpc` obtains an [`OverlaidProcessor`] via
//! [`Processor::overlaid`] for read-only reply queries. The processor itself is
//! never polled as a stream and emits no events. It builds on `ethexe-runtime`
//! (the embedded WASM), `ethexe-db` (the `Database` and its overlay), and
//! `ethexe-runtime-common` (transition controller and `FinalizedBlockTransitions`).
//!
//! ## Public API
//!
//! | Method | Purpose |
//! |--------|---------|
//! | [`Processor::process_code`] | Validate a WASM blob and, on match, instrument it into a [`ProcessedCodeInfo`]. |
//! | [`Processor::process_programs`] | Execute an ethexe block from [`ExecutableData`], returning `FinalizedBlockTransitions`. |
//! | [`Processor::overlaid`] | Wrap `self` into an [`OverlaidProcessor`] over a copy-on-write DB. |
//! | [`OverlaidProcessor::execute_for_reply`] | Simulate one incoming message and return its reply. |
//!
//! ## Key types
//!
//! - [`Processor`] — main engine; constructed via [`Processor::new`] or
//!   [`Processor::with_config`].
//! - [`OverlaidProcessor`] — wraps a [`Processor`] whose database is a copy-on-write
//!   overlay; mutations are discarded on drop and never reach the underlying DB.
//! - [`ExecutableData`] — full block input for [`Processor::process_programs`].
//! - [`ExecutableDataForReply`] / [`ExecuteForReplyOutcome`] — input and output for
//!   reply simulation.
//! - [`ProcessorConfig`] — single knob `chunk_size`; default is
//!   [`DEFAULT_CHUNK_SIZE`] (16).
//! - [`ProcessedCodeInfo`] — result of [`Processor::process_code`]; its `valid` field
//!   holds a [`ValidCodeInfo`] on success, or `None` if the code id does not match the
//!   hash or the code fails instrumentation (not an error in either case).
//! - [`ProcessorError`] / [`ExecuteForReplyError`] — the two public error types.
//! - [`BoundPromiseSink`] — receives promises emitted during queue execution.
//! - [`InstanceError`] — error from instantiating or calling the runtime WASM.
//!
//! ## Invariants
//!
//! - **Determinism** — block execution is a deterministic function of its inputs, so
//!   every node executing the same block arrives at the same state hashes.

pub use host::InstanceError;
pub use promise::BoundPromiseSink;

use core::num::NonZero;
use ethexe_common::{
    CodeAndIdUnchecked, ProgramStates, Schedule,
    ecdsa::VerifiedData,
    events::{BlockRequestEvent, MirrorRequestEvent, mirror::MessageQueueingRequestedEvent},
    gear::Message,
    injected::InjectedTransaction,
};
use ethexe_db::Database;
use ethexe_runtime_common::{
    FinalizedBlockTransitions, InBlockTransitions, ScheduleHandler, TransitionController,
    state::Storage,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::prelude::CodeIdExt,
    rpc::ReplyInfo,
};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use handling::{ProcessingHandler, overlaid::OverlaidRunContext, run::CommonRunContext};
use host::InstanceCreator;

mod handling;
mod host;
mod promise;

#[cfg(test)]
mod tests;
mod thread_pool;

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

    #[error("missing original code for code id {0}")]
    MissingOriginalCodeForProgram(CodeId),

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
        let creator = InstanceCreator::new(db.clone(), host::runtime())?;
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
        self.creator = self.creator.with_db(self.db.clone());

        OverlaidProcessor(self)
    }

    pub async fn process_code(
        &mut self,
        code_and_id: CodeAndIdUnchecked,
    ) -> Result<ProcessedCodeInfo> {
        log::debug!("Processing upload code {code_and_id:?}");

        let CodeAndIdUnchecked { code, code_id } = code_and_id;

        if code_id != CodeId::generate(&code) {
            return Ok(ProcessedCodeInfo {
                code_id,
                valid: None,
            });
        }

        let mut instance = self.creator.instantiate()?;
        let valid = thread_pool::spawn(move || -> Result<_> {
            let instrumented_code = instance.instrument(&code)?;
            let info = instrumented_code.map(|(instrumented_code, code_metadata)| ValidCodeInfo {
                code,
                instrumented_code,
                code_metadata,
            });
            Ok(info)
        })
        .await?;

        if let Some(valid) = &valid {
            let status = valid.code_metadata.instrumentation_status();
            assert!(
                status.is_instrumented(),
                "Instrumented code returned, but instrumentation status is not Instrumented: {status:?}"
            );
        }

        Ok(ProcessedCodeInfo { code_id, valid })
    }

    pub async fn process_programs(
        &mut self,
        executable: ExecutableData,
        promise_sink: Option<BoundPromiseSink>,
    ) -> Result<FinalizedBlockTransitions> {
        log::debug!("{executable}");

        let ExecutableData {
            height,
            timestamp,
            program_states,
            schedule,
            injected_transactions,
            gas_allowance,
            events,
        } = executable;

        let mut transitions = InBlockTransitions::new(height, program_states, schedule);

        // First step: push injected to queues and handle block events.
        transitions =
            self.handle_injected_and_events(transitions, injected_transactions, events)?;

        // Second step: process scheduled tasks.
        transitions = self.process_tasks(transitions);

        // Third step: process queues until limits are exhausted or all queues are empty.
        if let Some(gas_allowance) = gas_allowance {
            transitions = self
                .process_queues(transitions, height, timestamp, gas_allowance, promise_sink)
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
        height: u32,
        timestamp: u64,
        gas_allowance: u64,
        promise_sink: Option<BoundPromiseSink>,
    ) -> Result<InBlockTransitions> {
        CommonRunContext::new(
            self.db.clone(),
            self.creator.clone(),
            transitions,
            gas_allowance,
            self.config.chunk_size,
            height,
            timestamp,
            promise_sink,
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

#[derive(Debug, Clone, Default)]
pub struct ProcessedCodeInfo {
    pub code_id: CodeId,
    pub valid: Option<ValidCodeInfo>,
}

#[derive(Debug, Clone)]
pub struct ValidCodeInfo {
    pub code: Vec<u8>,
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
}

#[derive(Debug, derive_more::Display)]
#[display(
    "ExecutableData(height: {height}, timestamp: {timestamp}, programs: {}, \
    schedule len: {}, gas_allowance: {gas_allowance:?}, injected: {}, events: {})",
    program_states.len(), schedule.len(), injected_transactions.len(), events.len(),
)]
pub struct ExecutableData {
    pub height: u32,
    pub timestamp: u64,
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
            height: 0,
            timestamp: 0,
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
    "Execution for reply at height {height} timestamp {timestamp}: \
    program_id: {program_id}, source: {source}, payload len: {}, \
    value: {value}, gas_allowance: {gas_allowance}", payload.len()
)]
pub struct ExecutableDataForReply {
    pub height: u32,
    pub timestamp: u64,
    pub program_states: ProgramStates,
    pub source: ActorId,
    pub program_id: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub gas_allowance: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteForReplyOutcome {
    pub reply: ReplyInfo,
    pub messages: Vec<Message>,
}

#[derive(Clone, derive_more::AsRef, derive_more::AsMut)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    pub async fn execute_for_reply(
        &mut self,
        executable: ExecutableDataForReply,
    ) -> Result<ExecuteForReplyOutcome, ExecuteForReplyError> {
        log::debug!("{executable}");

        let ExecutableDataForReply {
            height,
            timestamp,
            program_states,
            source,
            program_id,
            payload,
            value,
            gas_allowance,
        } = executable;

        let known_programs = program_states.keys().copied().collect::<Vec<_>>();

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

        let transitions = InBlockTransitions::new(height, program_states, Schedule::default());

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
            height,
            timestamp,
        )
        .run()
        .await?;

        let mut reply = None;
        let mut messages = Vec::new();

        for (_, message) in transitions.current_messages() {
            if let Some(details) = &message.reply_details
                && details.to_message_id() == MessageId::zero()
            {
                reply = Some(ReplyInfo {
                    payload: message.payload,
                    value: message.value,
                    code: details.to_reply_code(),
                });
                continue;
            }

            if !known_programs.contains(&message.destination) {
                messages.push(message);
            }
        }

        Ok(ExecuteForReplyOutcome {
            reply: reply.ok_or(ExecuteForReplyError::ReplyNotFound)?,
            messages,
        })
    }
}
