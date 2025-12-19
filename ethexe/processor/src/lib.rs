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

//! Program's execution service for eGPU.

use crate::handling::run::RunContext;
use core::num::NonZero;
use ethexe_common::{
    Announce, CodeAndIdUnchecked, HashOf, ProgramStates, Schedule, SimpleBlockData,
    ecdsa::VerifiedData,
    events::{BlockRequestEvent, MirrorRequestEvent},
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
use handling::{
    ProcessingHandler,
    run::{CommonRunContext, OverlaidRunContext},
};
use host::InstanceCreator;

pub use common::LocalOutcome;

pub mod host;

mod common;
mod handling;

#[cfg(test)]
mod tests;

// Default amount of virtual threads to use for programs processing.
pub const DEFAULT_CHUNK_PROCESSING_THREADS: NonZero<usize> = NonZero::new(16).unwrap();

// Default block gas limit for the node.
pub const DEFAULT_BLOCK_GAS_LIMIT: u64 = 4_000_000_000_000;

// Default multiplier for the block gas limit in overlay execution.
pub const DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER: u64 = 10;

#[derive(thiserror::Error, Debug)]
pub enum ProcessorError {
    // `OverlaidProcessor` errors
    #[error("program isn't yet initialized")]
    ProgramNotInitialized,
    #[error("reply wasn't found")]
    ReplyNotFound,
    #[error("not found state hash for program ({0})")]
    StateNotFound(ActorId),
    #[error("not found program state by hash ({0}) in database")]
    StatePartiallyPresentsInStorage(H256),
    #[error("block is not synced ({0})")]
    BlockIsNotSynced(H256),
    #[error("not found program states for processing announce ({0})")]
    AnnounceProgramStatesNotFound(HashOf<Announce>),
    #[error("not found block start schedule for processing announce ({0})")]
    AnnounceScheduleNotFound(HashOf<Announce>),
    #[error("not found announce by hash ({0})")]
    AnnounceNotFound(HashOf<Announce>),

    // `InstanceWrapper` errors
    #[error("couldn't find 'memory' export")]
    MemoryExportNotFound,
    #[error("'memory' export is not a wasm memory")]
    InvalidMemory,
    #[error("couldn't find `__indirect_function_table` export")]
    IndirectFunctionTableNotFound,
    #[error("`__indirect_function_table` is not table")]
    InvalidIndirectFunctionTable,
    #[error("couldn't find `__heap_base` export")]
    HeapBaseNotFound,
    #[error("`__heap_base` is not global")]
    HeapBaseIsNotGlobal,
    #[error("`__heap_base` is not i32")]
    HeapBaseIsNotI32,
    #[error("failed to write call input: {0}")]
    CallInputWrite(String),
    #[error("host state should be set before call and reset after")]
    HostStateNotSet,
    #[error("allocator should be set after `set_host_state`")]
    AllocatorNotSet,

    // `ProcessingHandler` errors
    #[error("db corrupted: missing code [OR] code existence wasn't checked on Eth, code id: {0}")]
    MissingCode(CodeId),

    #[error("code id not found for created program {0}")]
    MissingCodeIdForProgram(ActorId),

    #[error("missing instrumented code for program {program_id} with code id {code_id}")]
    MissingInstrumentedCodeForProgram {
        program_id: ActorId,
        code_id: CodeId,
    },

    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    #[error("decoding runtime call output error: {0}")]
    CallOutput(#[from] parity_scale_codec::Error),

    #[error("sp allocator error: {0}")]
    SpAllocator(#[from] sp_allocator::Error),
}

pub(crate) type Result<T> = std::result::Result<T, ProcessorError>;

#[derive(Clone, Debug)]
pub struct ProcessorConfig {
    pub chunk_size: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_PROCESSING_THREADS.get(),
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

        let Some(instruction_weight_version) = code_metadata.instruction_weights_version() else {
            return Ok(ProcessedCodeInfo {
                code_id,
                valid: None,
            });
        };

        Ok(ProcessedCodeInfo {
            code_id,
            valid: Some(ValidCodeInfo {
                code,
                instrumented_code,
                code_metadata,
                instruction_weight_version,
            }),
        })
    }

    pub async fn process_programs(
        &mut self,
        executable: ExecutableData,
    ) -> Result<FinalizedBlockTransitions> {
        let ExecutableData {
            block,
            program_states,
            schedule,
            injected_transactions,
            gas_allowance,
            events,
        } = executable;

        let injected_messages = injected_transactions
            .iter()
            .map(|tx| tx.data().to_message_id())
            .collect();

        let transitions = InBlockTransitions::new(
            block.header.height,
            program_states,
            schedule,
            injected_messages,
        );

        let transitions = self.process_tasks(transitions);
        let transitions =
            self.process_injected_and_events(transitions, injected_transactions, events)?;
        let transitions = self.process_queues(transitions, block, gas_allowance).await;

        Ok(transitions.finalize())
    }

    fn process_injected_and_events(
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
        mut transitions: InBlockTransitions,
        block: SimpleBlockData,
        gas_allowance: Option<u64>,
    ) -> InBlockTransitions {
        let Some(gas_allowance) = gas_allowance else {
            return transitions;
        };

        self.creator.set_chain_head(block);

        CommonRunContext::new(
            self.db.clone(),
            self.creator.clone(),
            &mut transitions,
            gas_allowance,
            self.config.chunk_size,
        )
        .run()
        .await;

        transitions
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

pub struct ProcessedCodeInfo {
    pub code_id: CodeId,
    pub valid: Option<ValidCodeInfo>,
}

pub struct ValidCodeInfo {
    pub code: Vec<u8>,
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
    pub instruction_weight_version: u32,
}

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
            gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
            events: vec![],
        }
    }
}

pub struct ExecutableDataForReply {
    pub block: SimpleBlockData,
    pub program_states: ProgramStates,
    pub source: ActorId,
    pub program_id: ActorId,
    pub payload: Vec<u8>,
    pub gas_limit: u64,
}

#[derive(Clone)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    pub async fn execute_for_reply(
        &mut self,
        executable: ExecutableDataForReply,
    ) -> Result<ReplyInfo> {
        let ExecutableDataForReply {
            block,
            program_states,
            source,
            program_id,
            payload,
            gas_limit,
        } = executable;

        self.0.creator.set_chain_head(block);

        let state_hash = program_states
            .get(&program_id)
            .ok_or(ProcessorError::StateNotFound(program_id))?
            .hash;

        let state = self
            .0
            .db
            .program_state(state_hash)
            .ok_or(ProcessorError::StatePartiallyPresentsInStorage(state_hash))?;

        if state.requires_init_message() {
            return Err(ProcessorError::ProgramNotInitialized);
        }

        let transitions = InBlockTransitions::new(
            block.header.height,
            program_states,
            Schedule::default(),
            Default::default(),
        );

        let mut transitions = self.0.process_injected_and_events(
            transitions,
            vec![],
            vec![BlockRequestEvent::Mirror {
                actor_id: program_id,
                event: MirrorRequestEvent::MessageQueueingRequested {
                    id: MessageId::zero(),
                    source,
                    payload: payload.clone(),
                    value: 0,
                    call_reply: true,
                },
            }],
        )?;

        OverlaidRunContext::new(
            program_id,
            self.0.db.clone(),
            &mut transitions,
            gas_limit,
            self.0.config.chunk_size,
            self.0.creator.clone(),
        )
        .run()
        .await;

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
            .ok_or(ProcessorError::ReplyNotFound)?;

        Ok(res)
    }
}
