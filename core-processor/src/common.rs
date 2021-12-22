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
    collections::{BTreeMap, BTreeSet, VecDeque},
    vec::Vec,
};
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};

#[derive(Clone, Copy, Debug)]
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

    pub outgoing: Vec<Message>,
    pub awakening: Vec<MessageId>,

    pub gas_left: u64,
    pub gas_burned: u64,

    pub page_update: BTreeMap<PageNumber, Vec<u8>>,
    pub nonce: u64,
}

impl DispatchResult {
    pub fn message_id(&self) -> MessageId {
        self.dispatch.message.id()
    }

    pub fn program_id(&self) -> ProgramId {
        self.program.id()
    }

    pub fn message_source(&self) -> ProgramId {
        self.dispatch.message.source()
    }

    pub fn trap_reply(&mut self) -> Option<Message> {
        if let Some((_, exit_code)) = self.dispatch.message.reply() {
            if exit_code != 0 {
                return None;
            }
        };

        let nonce = self.nonce;
        self.nonce += 1;

        let message = Message::new_reply(
            crate::id::next_message_id(self.program_id(), nonce),
            self.program_id(),
            self.message_source(),
            Default::default(),
            self.gas_left,
            0,
            self.message_id(),
            crate::ERR_EXIT_CODE,
        );

        self.gas_left = 0;

        Some(message)
    }
}

#[derive(Clone, Debug)]
pub enum DispatchOutcome {
    InitSuccess {
        message_id: MessageId,
        origin: ProgramId,
        program: Program,
    },
    InitFailure {
        message_id: MessageId,
        origin: ProgramId,
        program_id: ProgramId,
        reason: &'static str,
    },
    MessageTrap {
        message_id: MessageId,
        trap: Option<&'static str>,
    },
    Success(MessageId),
}

#[derive(Clone, Debug)]
pub enum JournalNote {
    MessageDispatched(DispatchOutcome),
    GasBurned {
        message_id: MessageId,
        origin: ProgramId,
        amount: u64,
    },
    MessageConsumed(MessageId),
    SendMessage {
        message_id: MessageId,
        message: Message,
    },
    WaitDispatch(Dispatch),
    WakeMessage {
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    },
    UpdateNonce {
        program_id: ProgramId,
        nonce: u64,
    },
    UpdatePage {
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    },
}

pub trait JournalHandler {
    fn message_dispatched(&mut self, outcome: DispatchOutcome);
    fn gas_burned(&mut self, message_id: MessageId, origin: ProgramId, amount: u64);
    fn message_consumed(&mut self, message_id: MessageId);
    fn send_message(&mut self, message_id: MessageId, message: Message);
    fn wait_dispatch(&mut self, dispatch: Dispatch);
    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    );
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

#[derive(Clone, Default)]
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
            .field("programs", &self.programs.iter().map(|(id, prog)| (*id, prog.get_pages().keys().cloned().collect::<BTreeSet<PageNumber>>())).collect::<BTreeMap<ProgramId, BTreeSet<PageNumber>>>())
            .field("current_failed", &self.current_failed)
            .finish()
    }
}

pub trait CollectState {
    fn collect(&self) -> State;
}
