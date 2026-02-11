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
    CodeAndIdUnchecked, ProgramStates, Schedule, SimpleBlockData,
    ecdsa::VerifiedData,
    events::{BlockRequestEvent, MirrorRequestEvent, mirror::MessageQueueingRequestedEvent},
    injected::InjectedTransaction,
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

        let injected_messages = injected_transactions
            .iter()
            .map(|tx| tx.data().to_message_id());

        let mut transitions = InBlockTransitions::new(
            block.header.height,
            program_states,
            schedule,
            injected_messages,
        );

        transitions =
            self.process_injected_and_events(transitions, injected_transactions, events)?;
        if let Some(gas_allowance) = gas_allowance {
            transitions = self
                .process_queues(transitions, block, gas_allowance)
                .await?;
        }
        transitions = self.process_tasks(transitions);

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
        transitions: InBlockTransitions,
        block: SimpleBlockData,
        gas_allowance: u64,
    ) -> Result<InBlockTransitions> {
        self.creator.set_chain_head(block);

        CommonRunContext::new(
            self.db.clone(),
            self.creator.clone(),
            transitions,
            gas_allowance,
            self.config.chunk_size,
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

pub struct ProcessedCodeInfo {
    pub code_id: CodeId,
    pub valid: Option<ValidCodeInfo>,
}

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

        self.0.creator.set_chain_head(block);

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

        let transitions = InBlockTransitions::new(
            block.header.height,
            program_states,
            Schedule::default(),
            vec![],
        );

        let transitions = self.0.process_injected_and_events(
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
