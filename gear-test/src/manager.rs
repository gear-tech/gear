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

use core_processor::common::*;
use gear_core::{
    memory::PageNumber,
    message::{Dispatch, DispatchKind, Message, MessageId},
    program::{Program, ProgramId},
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use crate::check::ExecutionContext;

#[derive(Clone, Default)]
pub struct InMemoryExtManager {
    message_queue: VecDeque<Message>,
    log: Vec<Message>,
    programs: RefCell<BTreeMap<ProgramId, Option<Program>>>,
    waiting_init: RefCell<BTreeMap<ProgramId, Vec<MessageId>>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,
    current_failed: bool,
}

impl InMemoryExtManager {
    fn move_waiting_msgs_to_mq(&mut self, program_id: ProgramId) {
        let waiting_messages = self.waiting_init.borrow_mut().remove(&program_id);
        for m_id in waiting_messages.iter().flatten() {
            if let Some(msg) = self.wait_list.remove(&(program_id, *m_id)) {
                self.message_queue.push_back(msg);
            }
        }
    }
}

impl ExecutionContext for InMemoryExtManager {
    fn store_program(&self, program: gear_core::program::Program, _init_message_id: MessageId) {
        self.waiting_init.borrow_mut().insert(program.id(), vec![]);
        self.programs.borrow_mut().insert(program.id(), Some(program));
    }

    fn message_to_dispatch(&self, message: Message) -> Dispatch {
        Dispatch {
            kind: if message.reply.is_some() {
                DispatchKind::HandleReply
            } else if self.waiting_init.borrow().contains_key(&message.dest()) {
                DispatchKind::Init
            } else {
                DispatchKind::Handle
            },
            message,
        }
    }
}

impl CollectState for InMemoryExtManager {
    fn collect(&self) -> State {
        let InMemoryExtManager {
            message_queue,
            log,
            programs,
            current_failed,
            ..
        } = self.clone();

        let programs = programs.
            into_inner()
            .into_iter()
            .filter_map(|(id, p_opt)| p_opt.map(|p| (id, p)))
            .collect();

        State {
            message_queue,
            log,
            programs,
            current_failed,
        }
    }
}

impl JournalHandler for InMemoryExtManager {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        self.current_failed = match outcome {
            DispatchOutcome::MessageTrap { .. } => true,
            DispatchOutcome::InitFailure { program_id, .. } => {
                self.move_waiting_msgs_to_mq(program_id);
                if let Some(prog) = self.programs.borrow_mut().get_mut(&program_id) {
                    // Program is now considered terminated (in opposite to active). But not deleted from the state.
                    *prog = None;
                }
                true
            },
            DispatchOutcome::Success(_) | DispatchOutcome::Skip(_) => false,
            DispatchOutcome::InitSuccess { program_id, .. } => {
                self.move_waiting_msgs_to_mq(program_id);
                false
            }
        };
    }
    fn gas_burned(&mut self, _message_id: MessageId, _origin: ProgramId, _amount: u64) {}
    fn message_consumed(&mut self, message_id: MessageId) {
        if let Some(index) = self
            .message_queue
            .iter()
            .position(|msg| msg.id() == message_id)
        {
            self.message_queue.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, dispatch: Dispatch) {
        let Dispatch { message, .. } = dispatch;
        let id = message.dest();
        if self.programs.borrow().contains_key(&id) {
            let mut borrowed_list = self.waiting_init.borrow_mut();
            if let (None, Some(list)) = (message.reply(), borrowed_list.get_mut(&id)) {
                list.push(message.id);
                self.wait_list.insert((id, message.id), message);
            } else {
                self.message_queue.push_back(message);
            }
        } else {
            self.log.push(message);
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.message_consumed(dispatch.message.id());
        self.wait_list.insert(
            (dispatch.message.dest(), dispatch.message.id()),
            dispatch.message,
        );
    }
    fn wake_message(
        &mut self,
        _message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        if let Some(msg) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.message_queue.push_back(msg);
        }
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        let mut programs = self.programs.borrow_mut();
        if let Some(prog) = programs.get_mut(&program_id).expect("Program not found in storage") {
            prog.set_message_nonce(nonce);
        } else {
            // panic!("Can't update nonce for terminated program");
        }
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        let mut programs = self.programs.borrow_mut();
        if let Some(prog) = programs.get_mut(&program_id).expect("Program not found in storage") {
            if let Some(data) = data {
                let _ = prog.set_page(page_number, &data);
            } else {
                prog.remove_page(page_number);
            }
        } else {
            // panic!("Can't update page for terminated program");
        }
    }
    fn send_value(&mut self, _from: ProgramId, _to: Option<ProgramId>, _value: u128) {
        todo!("TODO https://github.com/gear-tech/gear/issues/644")
    }
}
