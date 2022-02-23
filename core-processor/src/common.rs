// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
    vec::Vec,
};
use codec::{Decode, Encode};
use gear_core::{
    gas::GasAmount,
    memory::PageNumber,
    message::{Dispatch, Message, MessageId},
    program::{CodeHash, Program, ProgramId},
};

/// Kind of the dispatch result.
#[derive(Clone)]
pub enum DispatchResultKind {
    /// Successful dispatch
    Success,
    /// Trap dispatch.
    Trap(Option<&'static str>),
    /// Wait dispatch.
    Wait,
    /// Exit dispatch.
    Exit(ProgramId),
}

/// Result of the specific dispatch.
pub struct DispatchResult {
    /// Kind of the dispatch.
    pub kind: DispatchResultKind,

    /// Original dispatch.
    pub dispatch: Dispatch,

    /// List of generated from program messages.
    pub generated_dispatches: Vec<Dispatch>,
    /// List of messages that should be woken.
    pub awakening: Vec<MessageId>,

    /// New programs to be created with additional data (corresponding code hash and init message id).
    pub program_candidates_data: BTreeMap<CodeHash, Vec<(ProgramId, MessageId)>>,

    /// Gas amount after execution.
    pub gas_amount: GasAmount,

    /// Page updates.
    pub page_update: BTreeMap<PageNumber, Option<Vec<u8>>>,
    /// New nonce.
    pub nonce: u64,
}

impl DispatchResult {
    /// Return dispatch message id.
    pub fn message_id(&self) -> MessageId {
        self.dispatch.message.id()
    }

    /// Return dispatch target program id.
    pub fn program_id(&self) -> ProgramId {
        self.dispatch.message.dest()
    }

    /// Return dispatch source program id.
    pub fn message_source(&self) -> ProgramId {
        self.dispatch.message.source()
    }

    /// Return dispatch message value
    pub fn message_value(&self) -> u128 {
        self.dispatch.message.value()
    }
}

/// Dispatch outcome of the specific message.
#[derive(Clone, Debug)]
pub enum DispatchOutcome {
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
        reason: &'static str,
    },
    /// Message was a trap.
    MessageTrap {
        /// Message id.
        message_id: MessageId,
        /// Program that was failed initializing.
        program_id: ProgramId,
        /// Reason of the fail.
        trap: Option<&'static str>,
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
    WaitDispatch(Dispatch),
    /// Wake particular message.
    WakeMessage {
        /// Message which has initiated wake.
        message_id: MessageId,
        /// Program which has initiated wake.
        program_id: ProgramId,
        /// Message that should be wokoen.
        awakening_id: MessageId,
    },
    /// Update program nonce.
    UpdateNonce {
        /// Program id to be updated.
        program_id: ProgramId,
        /// Nonce to set.
        nonce: u64,
    },
    /// Update page.
    UpdatePage {
        /// Program that owns the page.
        program_id: ProgramId,
        /// Number of the page.
        page_number: PageNumber,
        /// New data of the page.
        ///
        /// Updates data in case of `Some(data)` or deletes the page
        data: Option<Vec<u8>>,
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
        code_hash: CodeHash,
        /// Collection of program candidate ids and their init message ids.
        candidates: Vec<(ProgramId, MessageId)>,
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
    fn wait_dispatch(&mut self, dispatch: Dispatch);
    /// Process send message.
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    );
    /// Process nonce update.
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64);
    /// Process page update.
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    );
    /// Send value
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128);
    /// Store new programs in storage
    ///
    /// Program ids are ids of_potential_ (planned to be initialized) programs.
    fn store_new_programs(&mut self, code_hash: CodeHash, candidates: Vec<(ProgramId, MessageId)>);
}

/// Execution error.
pub struct ExecutionError {
    /// Id of the program that generated execution error.
    pub program_id: ProgramId,
    /// Gas amount of the execution.
    pub gas_amount: GasAmount,
    /// Error text.
    pub reason: &'static str,
}

/// Executable actor.
#[derive(Clone, Debug, Decode, Encode)]
pub struct ExecutableActor {
    /// Program.
    pub program: Program,
    /// Program value balance.
    pub balance: u128,
}

#[derive(Clone, Default)]
/// In-memory state.
pub struct State {
    /// Message queue.
    pub dispatch_queue: VecDeque<Dispatch>,
    /// Log records.
    pub log: Vec<Message>,
    /// State of each executable actor.
    pub actors: BTreeMap<ProgramId, ExecutableActor>,
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
                    .map(|(id, ExecutableActor { program, balance })| {
                        (
                            *id,
                            (
                                *balance,
                                program
                                    .get_pages()
                                    .keys()
                                    .cloned()
                                    .collect::<BTreeSet<PageNumber>>(),
                            ),
                        )
                    })
                    .collect::<BTreeMap<ProgramId, (u128, BTreeSet<PageNumber>)>>(),
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
