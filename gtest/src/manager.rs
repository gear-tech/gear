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

use core_processor::common::*;
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use crate::check::ProgramStorage;

#[derive(Clone, Default)]
pub struct InMemoryExtManager {
    message_queue: VecDeque<Message>,
    log: Vec<Message>,
    programs: RefCell<BTreeMap<ProgramId, Program>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,
    current_failed: bool,
}

impl ProgramStorage for InMemoryExtManager {
    fn store_program(&self, program: gear_core::program::Program, _init_message_id: MessageId) {
        let _ = self.programs.borrow_mut().insert(program.id(), program);
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

        State {
            message_queue,
            log,
            programs: programs.into_inner(),
            current_failed,
        }
    }
}

impl JournalHandler for InMemoryExtManager {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        self.current_failed = matches!(
            outcome,
            DispatchOutcome::MessageTrap { .. } | DispatchOutcome::InitFailure { .. }
        );
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
    fn send_message(&mut self, _message_id: MessageId, message: Message) {
        if self.programs.borrow().contains_key(&message.dest()) {
            self.message_queue.push_back(message);
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
        if let Some(prog) = self.programs.borrow_mut().get_mut(&program_id) {
            prog.set_message_nonce(nonce);
        } else {
            panic!("Program not found in storage");
        }
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        if let Some(prog) = self.programs.borrow_mut().get_mut(&program_id) {
            if let Some(data) = data {
                let _ = prog.set_page(page_number, &data);
            } else {
                prog.remove_page(page_number);
            }
        } else {
            panic!("Program not found in storage");
        }
    }
}
