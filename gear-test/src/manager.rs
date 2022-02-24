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
    program::{CodeHash, Program, ProgramId},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::check::ExecutionContext;

#[derive(Clone, Default)]
pub struct InMemoryExtManager {
    codes: BTreeMap<CodeHash, Vec<u8>>,
    marked_destinations: BTreeSet<ProgramId>,
    dispatch_queue: VecDeque<Dispatch>,
    log: Vec<Message>,
    actors: BTreeMap<ProgramId, Option<ExecutableActor>>,
    waiting_init: BTreeMap<ProgramId, Vec<MessageId>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Dispatch>,
    current_failed: bool,
}

impl InMemoryExtManager {
    fn move_waiting_msgs_to_queue(&mut self, program_id: ProgramId) {
        let waiting_messages = self.waiting_init.remove(&program_id);
        for m_id in waiting_messages.iter().flatten() {
            if let Some(dispatch) = self.wait_list.remove(&(program_id, *m_id)) {
                self.dispatch_queue.push_back(dispatch);
            }
        }
    }
}

impl ExecutionContext for InMemoryExtManager {
    fn store_program(&mut self, program: gear_core::program::Program, _init_message_id: MessageId) {
        self.waiting_init.insert(program.id(), vec![]);
        let code_hash = sp_io::hashing::blake2_256(program.code()).into();
        self.codes.insert(code_hash, program.code().to_vec());
        self.actors.insert(
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
            .into_iter()
            .filter_map(|(id, a_opt)| a_opt.map(|a| (id, a)))
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
                if let Some(actor) = self.actors.get_mut(&program_id) {
                    // Program is now considered terminated (in opposite to active). But not deleted from the state.
                    *actor = None;
                }
                true
            }
            DispatchOutcome::Success(_) | DispatchOutcome::NoExecution(_) => false,
            DispatchOutcome::InitSuccess { program_id, .. } => {
                if let Some(Some(actor)) = self.actors.get_mut(&program_id) {
                    actor.program.set_initialized();
                }
                self.move_waiting_msgs_to_queue(program_id);
                false
            }
        };
    }
    fn gas_burned(&mut self, _message_id: MessageId, _amount: u64) {}

    fn exit_dispatch(&mut self, id_exited: ProgramId, _value_destination: ProgramId) {
        self.actors.remove(&id_exited);
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
        if self.actors.contains_key(&dest) || self.marked_destinations.contains(&dest) {
            // imbuing gas-less messages with maximum gas!
            if dispatch.message.gas_limit.is_none() {
                dispatch.message.gas_limit = Some(u64::max_value());
            }

            // Find in dispatch queue init message to the destination. By that we recognize
            // messages to not yet initialized programs, whose init messages weren't executed yet.
            let init_to_dest = self
                .dispatch_queue
                .iter()
                .find(|d| d.message.dest() == dest && d.kind == DispatchKind::Init);
            if let (DispatchKind::Handle, Some(list), None) = (
                dispatch.kind,
                self.waiting_init.get_mut(&dest),
                init_to_dest,
            ) {
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
        if let Some(actor) = self
            .actors
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
        if let Some(actor) = self
            .actors
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
            if let Some(Some(actor)) = self.actors.get_mut(&from) {
                if actor.balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                actor.balance -= value;
            };

            if let Some(Some(actor)) = self.actors.get_mut(&to) {
                actor.balance += value;
            };
        };
    }

    fn store_new_programs(&mut self, code_hash: CodeHash, candidates: Vec<(ProgramId, MessageId)>) {
        if let Some(code) = self.codes.get(&code_hash).cloned() {
            for (candidate_id, init_message_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let program = Program::new(candidate_id, code.clone())
                        .expect("guaranteed to have constructable code");
                    self.store_program(program, init_message_id);
                } else {
                    log::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_hash
            );
            for (invalid_candidate, _) in candidates {
                self.marked_destinations.insert(invalid_candidate);
            }
        }
    }
}
