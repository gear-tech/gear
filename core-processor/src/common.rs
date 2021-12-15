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

use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};

use crate::{executor::ERR_EXIT_CODE, id};

use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};

#[derive(Clone, Debug)]
pub enum DispatchKind {
    Init,
    Handle,
    HandleReply,
}

impl DispatchKind {
    pub fn into_entry(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::HandleReply => "handle_reply",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Dispatch {
    pub kind: DispatchKind,
    pub message: Message,
}

#[derive(Clone)]
pub enum DispatchResultKind {
    Success,
    Trap(Option<&'static str>),
    Wait,
}

pub struct DispatchResult {
    pub kind: DispatchResultKind,

    pub program: Program,
    pub dispatch: Dispatch,

    pub messages: Vec<Message>,
    pub awakening: Vec<MessageId>,

    pub gas_left: u64,
    pub gas_burned: u64,

    pub page_update: BTreeMap<PageNumber, Vec<u8>>,
    pub nonce: u64,
}

impl DispatchResult {
    pub fn program(&self) -> Program {
        self.program.clone()
    }

    pub fn message_source(&self) -> ProgramId {
        self.dispatch.message.source()
    }

    pub fn message_nonce(&self) -> u64 {
        self.nonce
    }

    pub fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.nonce;
        self.nonce += 1;
        nonce
    }

    pub fn apply_nonce(&mut self) {
        self.program.set_message_nonce(self.nonce)
    }

    pub fn program_id(&self) -> ProgramId {
        self.program.id()
    }

    pub fn message_id(&self) -> MessageId {
        self.dispatch.message.id()
    }

    pub fn dispatch(&self) -> Dispatch {
        self.dispatch.clone()
    }

    pub fn kind(&self) -> DispatchResultKind {
        self.kind.clone()
    }

    pub fn gas_left(&self) -> u64 {
        self.gas_left
    }

    pub fn gas_burned(&self) -> u64 {
        self.gas_burned
    }

    pub fn outgoing(&self) -> Vec<Message> {
        self.messages.clone()
    }

    pub fn awakening(&self) -> Vec<MessageId> {
        self.awakening.clone()
    }

    pub fn page_update(&self) -> BTreeMap<PageNumber, Vec<u8>> {
        self.page_update.clone()
    }

    pub fn trap_reply(&mut self) -> Option<Message> {
        if let Some((_, exit_code)) = self.dispatch.message.reply() {
            if exit_code != 0 {
                return None;
            }
        };

        let message = Message::new_reply(
            id::next_message_id(self.program_id(), self.fetch_inc_message_nonce()),
            self.program_id(),
            self.dispatch.message.source(),
            Default::default(),
            self.gas_left(),
            0,
            self.message_id(),
            ERR_EXIT_CODE,
        );

        self.gas_burned += self.gas_left();
        self.gas_left = 0;

        Some(message)
    }
}

#[derive(Clone, Debug)]
pub enum JournalNote {
    ExecutionFail {
        origin: MessageId,
        initiator: ProgramId,
        program_id: ProgramId,
        reason: &'static str,
        entry: DispatchKind,
    },
    GasBurned {
        origin: MessageId,
        amount: u64,
    },
    MessageConsumed(MessageId),
    SendMessage {
        origin: MessageId,
        message: Message,
    },
    SubmitProgram {
        origin: MessageId,
        owner: ProgramId,
        program: Program,
    },
    WaitDispatch(Dispatch),
    WakeMessage {
        origin: MessageId,
        program_id: ProgramId,
        message_id: MessageId,
    },
    UpdateNonce {
        origin: MessageId,
        program_id: ProgramId,
        nonce: u64,
    },
    UpdatePage {
        origin: MessageId,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    },
    MessageTrap {
        origin: MessageId,
        trap: Option<&'static str>,
    },
}

pub trait JournalHandler {
    fn execution_fail(
        &mut self,
        origin: MessageId,
        initiator: ProgramId,
        program_id: ProgramId,
        reason: &'static str,
        entry: DispatchKind,
    );
    fn gas_burned(&mut self, origin: MessageId, amount: u64);
    fn message_consumed(&mut self, message_id: MessageId);
    fn message_trap(&mut self, origin: MessageId, trap: Option<&'static str>);
    fn send_message(&mut self, origin: MessageId, message: Message);
    fn submit_program(&mut self, origin: MessageId, owner: ProgramId, program: Program);
    fn wait_dispatch(&mut self, dispatch: Dispatch);
    fn wake_message(&mut self, origin: MessageId, program_id: ProgramId, message_id: MessageId);
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64);
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>);
}

pub struct ProcessResult {
    pub program: Program,
    pub journal: Vec<JournalNote>,
}

pub struct ExecutionError {
    pub program: Program,
    pub gas_burned: u64,
    pub reason: &'static str,
}

#[derive(Clone)]
pub struct State {
    pub message_queue: VecDeque<Message>,
    pub log: Vec<Message>,
    pub programs: BTreeMap<ProgramId, Program>,
    pub current_failed: bool,
}

impl alloc::fmt::Debug for State {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        f.debug_struct("State")
            .field("message_queue", &self.message_queue)
            .field("log", &self.log)
            .field("programs", &self.programs.keys())
            .field("current_failed", &self.current_failed)
            .finish()
    }
}

pub trait CollectState {
    fn collect(&self) -> State;
}
