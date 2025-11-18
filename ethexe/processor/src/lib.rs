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

use core::num::NonZero;
use ethexe_common::{
    Announce, CodeAndIdUnchecked, HashOf,
    db::{AnnounceStorageRO, AnnounceStorageRW, CodesStorageRW},
    events::{BlockRequestEvent, MirrorRequestEvent},
};
use ethexe_db::Database;
use ethexe_runtime_common::{FinalizedBlockTransitions, state::Storage};
use gear_core::{ids::prelude::CodeIdExt, rpc::ReplyInfo};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use handling::{
    ProcessingHandler,
    run::{self, CommonRunContext, OverlaidRunContext},
};
use host::InstanceCreator;

pub use common::LocalOutcome;
pub use handling::run::RunnerConfig;

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
    #[error("not found state for program ({program_id}) at announce ({announce_hash})")]
    StateNotFound {
        program_id: ActorId,
        announce_hash: HashOf<Announce>,
    },
    #[error("unreachable: state partially presents in storage")]
    StatePartiallyPresentsInStorage,
    #[error("not found header for processing block ({0})")]
    BlockHeaderNotFound(H256),
    #[error("not found program states for processing announce ({0})")]
    AnnounceProgramStatesNotFound(HashOf<Announce>),
    #[error("not found block start schedule for processing announce ({0})")]
    AnnounceScheduleNotFound(HashOf<Announce>),
    #[error("not found announce by hash ({0})")]
    AnnounceNotFound(HashOf<Announce>),

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

    #[error(transparent)]
    Wasm(#[from] wasmtime::Error),

    #[error(transparent)]
    ParityScaleCodes(#[from] parity_scale_codec::Error),

    #[error(transparent)]
    SpAllocator(#[from] sp_allocator::Error),
}

pub(crate) type Result<T> = std::result::Result<T, ProcessorError>;

#[derive(Clone, Debug)]
pub struct ProcessorConfig {
    pub chunk_processing_threads: usize,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            chunk_processing_threads: DEFAULT_CHUNK_PROCESSING_THREADS.get(),
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

    pub fn process_upload_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<bool> {
        log::debug!("Processing upload code {code_and_id:?}");

        let CodeAndIdUnchecked { code, code_id } = code_and_id;

        let valid = code_id == CodeId::generate(&code) && self.handle_new_code(code)?.is_some();

        self.db.set_code_valid(code_id, valid);

        Ok(valid)
    }

    pub async fn process_announce(
        &mut self,
        announce: Announce,
        events: Vec<BlockRequestEvent>,
    ) -> Result<FinalizedBlockTransitions> {
        log::debug!(
            "Processing events for {:?}: {events:#?}",
            announce.block_hash
        );

        // TODO kuzmindev: remove clone here
        let mut handler = self.handler(announce.clone())?;

        for tx in announce.injected_transactions {
            handler.handle_injected_transaction(tx)?;
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

        self.process_queue(&mut handler).await;

        handler.run_schedule();

        Ok(handler.transitions.finalize())
    }

    pub async fn process_queue(&mut self, handler: &mut ProcessingHandler) {
        let Some(block_gas_limit) = handler.announce.gas_allowance else {
            return;
        };

        self.creator.set_chain_head(handler.announce.block_hash);

        let ctx = CommonRunContext::new(&mut handler.transitions);
        let run_config =
            RunnerConfig::common(self.config().chunk_processing_threads, block_gas_limit);

        run::run(ctx, self.db.clone(), self.creator.clone(), run_config).await;
    }
}

#[derive(Clone)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    // TODO (breathx): optimize for one single program.
    pub async fn execute_for_reply(
        &mut self,
        announce_hash: HashOf<Announce>,
        source: ActorId,
        program_id: ActorId,
        payload: Vec<u8>,
        value: u128,
        runner_config: RunnerConfig,
    ) -> Result<ReplyInfo> {
        let block_hash = self
            .0
            .db
            .announce(announce_hash)
            .ok_or(ProcessorError::AnnounceNotFound(announce_hash))?
            .block_hash;
        self.0.creator.set_chain_head(block_hash);

        let announce = self
            .0
            .db
            .announce(announce_hash)
            .ok_or(ProcessorError::AnnounceNotFound(announce_hash))?;

        let mut handler = self.0.handler(announce)?;

        let state_hash = handler
            .transitions
            .state_of(&program_id)
            .ok_or(ProcessorError::StateNotFound {
                program_id,
                announce_hash,
            })?
            .hash;

        let state = handler
            .db
            .program_state(state_hash)
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

        let ctx = OverlaidRunContext::new(program_id, self.0.db.clone(), &mut handler.transitions);

        run::run_overlaid(
            ctx,
            self.0.db.clone(),
            self.0.creator.clone(),
            runner_config,
        )
        .await;

        // Getting message to users now, because later transitions are moved.
        let current_messages = handler.transitions.current_messages();

        // Setting program states and schedule for the block is not necessary, but important for testing.
        {
            let FinalizedBlockTransitions {
                states, schedule, ..
            } = handler.transitions.finalize();
            self.0.db.set_announce_program_states(announce_hash, states);
            self.0.db.set_announce_schedule(announce_hash, schedule);
        }

        let res = current_messages
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
