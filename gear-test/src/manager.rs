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
    dispatch_queue: VecDeque<Dispatch>,
    log: Vec<Message>,
    actors: RefCell<BTreeMap<ProgramId, Option<ExecutableActor>>>,
    waiting_init: RefCell<BTreeMap<ProgramId, Vec<MessageId>>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Dispatch>,
    current_failed: bool,
}

impl InMemoryExtManager {
    fn move_waiting_msgs_to_queue(&mut self, program_id: ProgramId) {
        let waiting_messages = self.waiting_init.borrow_mut().remove(&program_id);
        for m_id in waiting_messages.iter().flatten() {
            if let Some(dispatch) = self.wait_list.remove(&(program_id, *m_id)) {
                self.dispatch_queue.push_back(dispatch);
            }
        }
    }
}

impl ExecutionContext for InMemoryExtManager {
    fn store_program(&self, program: Program, _init_message_id: MessageId) {
        self.waiting_init.borrow_mut().insert(program.id(), vec![]);
        self.actors.borrow_mut().insert(
            program.id(),
            Some(ExecutableActor {
                program,
                balance: 0,
            }),
        );
    }
}

impl CollectState for InMemoryExtManager {
    fn collect(&self) -> State {
        let InMemoryExtManager {
            dispatch_queue,
            log,
            actors,
            current_failed,
            ..
        } = self.clone();

        let actors = actors
            .into_inner()
            .into_iter()
            .filter_map(|(id, p_opt)| p_opt.map(|p| (id, p)))
            .collect();

        State {
            dispatch_queue,
            log,
            actors,
            current_failed,
        }
    }
}

impl JournalHandler for InMemoryExtManager {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        self.current_failed = match outcome {
            DispatchOutcome::MessageTrap { .. } => true,
            DispatchOutcome::InitFailure { program_id, .. } => {
                self.move_waiting_msgs_to_queue(program_id);
                if let Some(actor) = self.actors.borrow_mut().get_mut(&program_id) {
                    // Program is now considered terminated (in opposite to active). But not deleted from the state.
                    *actor = None;
                }
                true
            }
            DispatchOutcome::Success(_) | DispatchOutcome::NoExecution(_) => false,
            DispatchOutcome::InitSuccess { program_id, .. } => {
                self.move_waiting_msgs_to_queue(program_id);
                false
            }
        };
    }
    fn gas_burned(&mut self, _message_id: MessageId, _amount: u64) {}

    fn exit_dispatch(&mut self, id_exited: ProgramId, _value_destination: ProgramId) {
        self.actors.borrow_mut().remove(&id_exited);
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        if let Some(index) = self
            .dispatch_queue
            .iter()
            .position(|d| d.message.id() == message_id)
        {
            self.dispatch_queue.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, mut dispatch: Dispatch) {
        let dest = dispatch.message.dest();
        if self.actors.borrow().contains_key(&dest) {

            // imbuing gas-less messages with maximum gas!
            if let None = dispatch.message.gas_limit {
                dispatch.message.gas_limit = Some(u64::max_value());
            }

            if let (DispatchKind::Handle, Some(list)) =
                (dispatch.kind, self.waiting_init.borrow_mut().get_mut(&dest))
            {
                let message_id = dispatch.message.id();
                list.push(message_id);
                self.wait_list.insert((dest, message_id), dispatch);
            } else {
                self.dispatch_queue.push_back(dispatch);
            }
        } else {
            self.log.push(dispatch.message);
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.message_consumed(dispatch.message.id());
        self.wait_list
            .insert((dispatch.message.dest(), dispatch.message.id()), dispatch);
    }
    fn wake_message(
        &mut self,
        _message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        if let Some(dispatch) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.dispatch_queue.push_back(dispatch);
        }
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        let mut programs = self.actors.borrow_mut();
        if let Some(actor) = programs
            .get_mut(&program_id)
            .expect("Program not found in storage")
        {
            actor.program.set_message_nonce(nonce);
        }
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        let mut actors = self.actors.borrow_mut();
        if let Some(actor) = actors
            .get_mut(&program_id)
            .expect("Program not found in storage")
        {
            if let Some(data) = data {
                let _ = actor.program.set_page(page_number, &data);
            } else {
                actor.program.remove_page(page_number);
            }
        } else {
            unreachable!("Can't update page for terminated program");
        }
    }
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        if let Some(to) = to {
            let mut actors = self.actors.borrow_mut();

            if let Some(Some(actor)) = actors.get_mut(&from) {
                if actor.balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                actor.balance -= value;
            };

            if let Some(Some(actor)) = actors.get_mut(&to) {
                actor.balance += value;
            };
        };
    }
}
