// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Common structures for processing.

use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{self, Debug, Formatter},
    string::String,
    vec::Vec,
};
use codec::{Decode, Encode};
use gear_backend_common::TrapExplanation;
use gear_core::{
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::{ContextStore, Dispatch, GasLimit, IncomingDispatch, StoredDispatch, StoredMessage},
    program::Program,
};
use gear_core_errors::MemoryError;

/// Kind of the dispatch result.
#[derive(Clone)]
pub enum DispatchResultKind {
    /// Successful dispatch
    Success,
    /// Trap dispatch.
    Trap(Option<TrapExplanation>),
    /// Wait dispatch.
    Wait,
    /// Exit dispatch.
    Exit(ProgramId),
    /// Gas allowance exceed.
    GasAllowanceExceed,
}

/// Result of the specific dispatch.
pub struct DispatchResult {
    /// Kind of the dispatch.
    pub kind: DispatchResultKind,
    /// Original dispatch.
    pub dispatch: IncomingDispatch,
    /// Program id of actor which was executed.
    pub program_id: ProgramId,
    /// Context store after execution.
    pub context_store: ContextStore,
    /// List of generated messages.
    pub generated_dispatches: Vec<Dispatch>,
    /// List of messages that should be woken.
    pub awakening: Vec<MessageId>,
    /// New programs to be created with additional data (corresponding code hash and init message id).
    pub program_candidates: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    /// Gas amount after execution.
    pub gas_amount: GasAmount,
    /// Page updates.
    pub page_update: BTreeMap<PageNumber, PageBuf>,
    /// New allocations set for program if it has been changed.
    pub allocations: Option<BTreeSet<WasmPageNumber>>,
}

impl DispatchResult {
    /// Return dispatch message id.
    pub fn message_id(&self) -> MessageId {
        self.dispatch.id()
    }

    /// Return program id.
    pub fn program_id(&self) -> ProgramId {
        self.program_id
    }

    /// Return dispatch source program id.
    pub fn message_source(&self) -> ProgramId {
        self.dispatch.source()
    }

    /// Return dispatch message value
    pub fn message_value(&self) -> u128 {
        self.dispatch.value()
    }
}

/// Dispatch outcome of the specific message.
#[derive(Clone, Debug)]
pub enum DispatchOutcome {
    /// Message was a exit.
    Exit {
        /// Message id.
        message_id: MessageId,
        /// Original actor.
        origin: ProgramId,
        /// Id of the program that was successfully exited.
        program_id: ProgramId,
    },
    /// Message was an initialization success.
    InitSuccess {
        /// Message id.
        message_id: MessageId,
        /// Original actor.
        origin: ProgramId,
        /// Id of the program that was successfully initialized.
        program_id: ProgramId,
    },
    /// Message was an initialization failure.
    InitFailure {
        /// Message id.
        message_id: MessageId,
        /// Original actor.
        origin: ProgramId,
        /// Program that was failed initializing.
        program_id: ProgramId,
        /// Reason of the fail.
        reason: Option<String>,
    },
    /// Message was a trap.
    MessageTrap {
        /// Message id.
        message_id: MessageId,
        /// Program that was failed initializing.
        program_id: ProgramId,
        /// Reason of the fail.
        trap: Option<String>,
    },
    /// Message was a success.
    Success(MessageId),
    /// Message was processed, but not executed
    NoExecution(MessageId),
}

/// Journal record for the state update.
#[derive(Clone, Debug)]
pub enum JournalNote {
    /// Message was successfully dispatched.
    MessageDispatched(DispatchOutcome),
    /// Some gas was burned.
    GasBurned {
        /// Message id in which gas was burned.
        message_id: MessageId,
        /// Amount of gas burned.
        amount: u64,
    },
    /// Exit the program.
    ExitDispatch {
        /// Id of the program called `exit`.
        id_exited: ProgramId,
        /// Address where all remaining value of the program should
        /// be transferred to.
        value_destination: ProgramId,
    },
    /// Message was handled and no longer exists.
    ///
    /// This should be the last update involving this message id.
    MessageConsumed(MessageId),
    /// Message was generated.
    SendDispatch {
        /// Message id of the message that generated this message.
        message_id: MessageId,
        /// New message with entry point that was generated.
        dispatch: Dispatch,
    },
    /// Put this dispatch in the wait list.
    WaitDispatch(StoredDispatch),
    /// Wake particular message.
    WakeMessage {
        /// Message which has initiated wake.
        message_id: MessageId,
        /// Program which has initiated wake.
        program_id: ProgramId,
        /// Message that should be wokoen.
        awakening_id: MessageId,
    },
    /// Update page.
    UpdatePage {
        /// Program that owns the page.
        program_id: ProgramId,
        /// Number of the page.
        page_number: PageNumber,
        /// New data of the page.
        data: PageBuf,
    },
    /// Update allocations set note.
    /// And also removes data for pages which is not in allocations set now.
    UpdateAllocations {
        /// Program id.
        program_id: ProgramId,
        /// New allocations set for the program.
        allocations: BTreeSet<WasmPageNumber>,
    },
    /// Send value
    SendValue {
        /// Value sender
        from: ProgramId,
        /// Value beneficiary,
        to: Option<ProgramId>,
        /// Value amount
        value: u128,
    },
    /// Store programs requested by user to be initialized later
    StoreNewPrograms {
        /// Code hash used to create new programs with ids in `candidates` field
        code_hash: CodeId,
        /// Collection of program candidate ids and their init message ids.
        candidates: Vec<(ProgramId, MessageId)>,
    },
    /// Stop processing queue.
    StopProcessing {
        /// Pushes StoredDispatch back to the top of the queue.
        dispatch: StoredDispatch,
        /// Decreases gas allowance by that amount, burned for processing try.
        gas_burned: u64,
    },
}

/// Journal handler.
///
/// Something that can update state.
pub trait JournalHandler {
    /// Process message dispatch.
    fn message_dispatched(&mut self, outcome: DispatchOutcome);
    /// Process gas burned.
    fn gas_burned(&mut self, message_id: MessageId, amount: u64);
    /// Process exit dispatch.
    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId);
    /// Process message consumed.
    fn message_consumed(&mut self, message_id: MessageId);
    /// Process send dispatch.
    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch);
    /// Process send message.
    fn wait_dispatch(&mut self, dispatch: StoredDispatch);
    /// Process send message.
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    );
    /// Process page update.
    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<PageNumber, PageBuf>,
    );
    /// Process [JournalNote::UpdateAllocations].
    fn update_allocations(&mut self, program_id: ProgramId, allocations: BTreeSet<WasmPageNumber>);
    /// Send value.
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128);
    /// Store new programs in storage.
    ///
    /// Program ids are ids of _potential_ (planned to be initialized) programs.
    fn store_new_programs(&mut self, code_hash: CodeId, candidates: Vec<(ProgramId, MessageId)>);
    /// Stop processing queue.
    ///
    /// Pushes StoredDispatch back to the top of the queue and decreases gas allowance.
    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64);
}

/// Execution error.
#[derive(Debug)]
pub struct ExecutionError {
    /// Id of the program that generated execution error.
    pub program_id: ProgramId,
    /// Gas amount of the execution.
    pub gas_amount: GasAmount,
    /// Error text.
    pub reason: ExecutionErrorReason,
}

/// Reason of execution error
#[derive(Debug, derive_more::Display)]
pub enum ExecutionErrorReason {
    /// Memory error
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    /// Backend error
    #[display(fmt = "{}", _0)]
    Backend(String),
    /// Processor error
    #[display(fmt = "{}", _0)]
    Processor(String),
    /// Ext error
    #[display(fmt = "{}", _0)]
    Ext(String),
    /// Program's max page is not last page in wasm page
    #[display(fmt = "Program's max page is not last page in wasm page")]
    NotLastPage,
    /// Not enough gas to load memory
    #[display(fmt = "Not enough gas to load memory")]
    LoadMemoryGasExceeded,
    /// Not enough gas in block to load memory
    #[display(fmt = "Not enough gas in block to load memory")]
    LoadMemoryBlockGasExceeded,
    /// Not enough gas to grow memory size
    #[display(fmt = "Not enough gas to grow memory size")]
    GrowMemoryGasExceeded,
    /// Not enough gas in block to grow memory size
    #[display(fmt = "Not enough gas in block to grow memory size")]
    GrowMemoryBlockGasExceeded,
    /// Not enough gas for initial memory handling
    #[display(fmt = "Not enough gas for initial memory handling")]
    InitialMemoryGasExceeded,
    /// Not enough gas in block for initial memory handling
    #[display(fmt = "Not enough gas in block for initial memory handling")]
    InitialMemoryBlockGasExceeded,
    /// Mem size less then static pages num
    #[display(fmt = "Mem size less then static pages num")]
    InsufficientMemorySize,
    /// Changed page has no data in initial pages
    #[display(fmt = "Changed page has no data in initial pages")]
    PageNoData,
    /// Ext works with lazy pages, but lazy pages env is not enabled
    #[display(fmt = "Ext works with lazy pages, but lazy pages env is not enabled")]
    LazyPagesInconsistentState,
    /// Page with data is not allocated for program
    #[display(fmt = "{:?} is not allocated for program", _0)]
    PageIsNotAllocated(PageNumber),
}

/// Executable actor.
#[derive(Clone, Debug, Decode, Encode)]
pub struct ExecutableActor {
    /// Program.
    pub program: Program,
    /// Program value balance.
    pub balance: u128,
    /// Data which some program allocated pages may have.
    pub pages_data: BTreeMap<PageNumber, PageBuf>,
}

/// Execution context.
#[derive(Clone, Debug, Decode, Encode)]
pub struct ExecutionContext {
    /// Original user.
    pub origin: ProgramId,
    /// Gas allowance of the block.
    pub gas_allowance: u64,
}

#[derive(Clone, Default)]
/// In-memory state.
pub struct State {
    /// Message queue.
    pub dispatch_queue: VecDeque<(StoredDispatch, GasLimit)>,
    /// Log records.
    pub log: Vec<StoredMessage>,
    /// State of each executable actor.
    pub actors: BTreeMap<ProgramId, Option<ExecutableActor>>,
    /// Is current state failed.
    pub current_failed: bool,
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("dispatch_queue", &self.dispatch_queue)
            .field("log", &self.log)
            .field(
                "actors",
                &self
                    .actors
                    .iter()
                    .filter_map(|(id, actor)| {
                        actor.as_ref().map(|actor| {
                            (
                                *id,
                                (actor.balance, actor.program.get_allocations().clone()),
                            )
                        })
                    })
                    .collect::<BTreeMap<ProgramId, (u128, BTreeSet<WasmPageNumber>)>>(),
            )
            .field("current_failed", &self.current_failed)
            .finish()
    }
}

/// Something that can return in-memory state.
pub trait CollectState {
    /// Collect the state from self.
    fn collect(&self) -> State;
}
