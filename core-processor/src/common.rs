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

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{executor::ERR_EXIT_CODE, id};

use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};

#[derive(Clone)]
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

#[derive(Clone)]
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
}

impl DispatchResult {
    pub fn program(self) -> Program {
        self.program
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

        Some(Message::new_reply(
            id::next_message_id(&mut self.program),
            self.program_id(),
            self.dispatch.message.source(),
            Default::default(),
            0,
            0,
            self.message_id(),
            ERR_EXIT_CODE,
        ))
    }
}

pub enum JournalNote {
    SendMessage {
        origin: MessageId,
        message: Message,
    },
    ExecutionFail {
        origin: MessageId,
        program_id: ProgramId,
        reason: &'static str,
    },
    WaitDispatch(Dispatch),
    MessageConsumed(MessageId),
    NotProcessed(Vec<Dispatch>),
    GasBurned {
        origin: MessageId,
        amount: u64,
    },
    WakeMessage {
        origin: MessageId,
        message_id: MessageId,
    },
    UpdatePage {
        origin: MessageId,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    },
}

pub trait JournalHandler {
    fn send_message(&mut self, origin: MessageId, message: Message);
    fn execution_fail(&mut self, origin: MessageId, program_id: ProgramId, reason: &'static str);
    fn wait_dispatch(&mut self, dispatch: Dispatch);
    fn message_consumed(&mut self, message_id: MessageId);
    fn not_processed(&mut self, dispatches: Vec<Dispatch>);
    fn gas_burned(&mut self, origin: MessageId, amount: u64);
    fn wake_message(&mut self, origin: MessageId, message_id: MessageId);
    fn update_page(
        &mut self,
        origin: MessageId,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    );
}

pub trait ResourceLimiter {
    fn can_process(&self, dispatch: &Dispatch) -> bool;
    fn pay_for(&mut self, dispatch: &Dispatch);
}

pub struct InfinityLimiter;

impl ResourceLimiter for InfinityLimiter {
    fn can_process(&self, _dispatch: &Dispatch) -> bool {
        true
    }
    fn pay_for(&mut self, _dispatch: &Dispatch) {}
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
