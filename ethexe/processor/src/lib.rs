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

use ethexe_common::{
    db::CodesStorageWrite,
    events::{BlockRequestEvent, MirrorRequestEvent},
    gear::StateTransition,
    ProgramStates, Schedule,
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gear_core::{ids::prelude::CodeIdExt, rpc::ReplyInfo};
use gprimitives::{ActorId, CodeId, MessageId, H256};
use handling::{run, ProcessingHandler};
use host::InstanceCreator;

pub use common::LocalOutcome;

pub mod host;

mod common;
mod handling;

#[cfg(test)]
mod tests;

#[derive(thiserror::Error, Debug)]
pub enum ProcessorError {
    // `OverlaidProcessor` errors
    #[error("program isn't yet initialized")]
    ProgramNotInitialized,
    #[error("reply wasn't found")]
    ReplyNotFound,
    #[error("not found state for program ({program_id}) at block ({block_hash})")]
    StateNotFound {
        program_id: ActorId,
        block_hash: H256,
    },
    #[error("unreachable: state partially presents in storage")]
    StatePartiallyPresentsInStorage,
    #[error("not found header for processing block ({0})")]
    BlockHeaderNotFound(H256),
    #[error("not found program states for processing block ({0})")]
    BlockProgramStatesNotFound(H256),
    #[error("not found block start schedule for processing block ({0})")]
    BlockScheduleNotFound(H256),

    // `InstanceWrapper` errors
    #[error("couldn't find 'memory' export")]
    MemoryExportNotFound,
    #[error("'memory' is not memory")]
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
    HeapBaseIsNoti32,
    #[error("failed to write call input: {0}")]
    CallInputWrite(String),
    #[error("host state should be set before call and reset after")]
    HostStateNotSet,
    #[error("allocator should be set after `set_host_state`")]
    AllocatorNotSet,

    // `ProcessingHandler` errors
    #[error("db corrupted: missing code [OR] code existence wasn't checked on Eth, code id: {0}")]
    MissingCode(CodeId),
    #[error("db corrupted: unrecognized program [OR] program duplicates wasn't checked on Eth, actor id: {0}")]
    DuplicatedProgram(ActorId),

    #[error(transparent)]
    Wasm(#[from] wasmtime::Error),

    #[error(transparent)]
    ParityScaleCodes(#[from] parity_scale_codec::Error),

    #[error(transparent)]
    SpAllocator(#[from] sp_allocator::Error),
}

pub(crate) type Result<T> = std::result::Result<T, ProcessorError>;

#[derive(Clone, Debug)]
pub struct BlockProcessingResult {
    pub transitions: Vec<StateTransition>,
    pub states: ProgramStates,
    pub schedule: Schedule,
}

#[derive(Clone, Debug)]
pub struct ProcessorConfig {
    pub chunk_processing_threads: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            chunk_processing_threads: 16,
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

    pub fn process_upload_code(
        &mut self,
        code_id: CodeId,
        code: &[u8],
    ) -> Result<Vec<LocalOutcome>> {
        let valid = self.process_upload_code_raw(code_id, code)?;

        Ok(vec![LocalOutcome::CodeValidated { id: code_id, valid }])
    }

    pub fn process_upload_code_raw(&mut self, code_id: CodeId, code: &[u8]) -> Result<bool> {
        log::debug!("Processing upload code {code_id:?}");

        let valid = code_id == CodeId::generate(code) && self.handle_new_code(code)?.is_some();

        self.db.set_code_validated(code_id, valid);

        Ok(valid)
    }

    pub async fn process_block_events(
        &mut self,
        block_hash: H256,
        events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        // Directly return the result from the raw processing function.
        // The caller (ComputeService) will now handle this result.
        self.process_block_events_raw(block_hash, events).await
    }

    pub async fn process_block_events_raw(
        &mut self,
        block_hash: H256,
        events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        log::debug!("Processing events for {block_hash:?}: {events:#?}");

        let mut handler = self.handler(block_hash)?;

        for event in events {
            match event {
                BlockRequestEvent::Router(event) => {
                    handler.handle_router_event(event)?;
                }
                BlockRequestEvent::Mirror { actor_id, event } => {
                    handler.handle_mirror_event(actor_id, event)?;
                }
                BlockRequestEvent::WVara(event) => {
                    handler.handle_wvara_event(event);
                }
            }
        }

        handler.run_schedule();
        self.process_queue(&mut handler).await;

        let (transitions, states, schedule) = handler.transitions.finalize();

        Ok(BlockProcessingResult {
            transitions,
            states,
            schedule,
        })
    }

    pub async fn process_queue(&mut self, handler: &mut ProcessingHandler) {
        self.creator.set_chain_head(handler.block_hash);

        run::run(
            self.config().chunk_processing_threads,
            self.db.clone(),
            self.creator.clone(),
            &mut handler.transitions,
        )
        .await;
    }
}

#[derive(Clone)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    // TODO (breathx): optimize for one single program.
    pub async fn execute_for_reply(
        &mut self,
        block_hash: H256,
        source: ActorId,
        program_id: ActorId,
        payload: Vec<u8>,
        value: u128,
    ) -> Result<ReplyInfo> {
        self.0.creator.set_chain_head(block_hash);

        let mut handler = self.0.handler(block_hash)?;

        let state_hash = handler
            .transitions
            .state_of(&program_id)
            .ok_or(ProcessorError::StateNotFound {
                program_id,
                block_hash,
            })?
            .hash;

        let state = handler
            .db
            .read_state(state_hash)
            .ok_or(ProcessorError::StatePartiallyPresentsInStorage)?;

        if state.requires_init_message() {
            return Err(ProcessorError::ProgramNotInitialized);
        }

        handler.handle_mirror_event(
            program_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::zero(),
                source,
                payload,
                value,
                call_reply: false,
            },
        )?;

        self.0.process_queue(&mut handler).await;

        let res = handler
            .transitions
            .current_messages()
            .into_iter()
            .find_map(|(_source, message)| {
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
