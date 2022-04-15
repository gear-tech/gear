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

use crate::check::ExecutionContext;
use core_processor::common::*;
use gear_core::{
    code::Code,
    ids::{CodeId, MessageId, ProgramId},
    memory::PageNumber,
    message::{Dispatch, DispatchKind, StoredDispatch, StoredMessage},
    program::Program,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Default)]
pub struct InMemoryExtManager {
    original_codes: BTreeMap<CodeId, Vec<u8>>,
    codes: BTreeMap<CodeId, Code>,
    marked_destinations: BTreeSet<ProgramId>,
    dispatch_queue: VecDeque<StoredDispatch>,
    log: Vec<StoredMessage>,
    actors: BTreeMap<ProgramId, Option<ExecutableActor>>,
    waiting_init: BTreeMap<ProgramId, Vec<MessageId>>,
    gas_limits: BTreeMap<MessageId, u64>,
    wait_list: BTreeMap<(ProgramId, MessageId), StoredDispatch>,
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
    fn store_code(&mut self, code_hash: CodeId, code: Code) {
        self.codes.insert(code_hash, code);
    }
    fn store_original_code(&mut self, code: &[u8]) {
        self.original_codes
            .insert(CodeId::generate(code), code.to_vec());
    }
    fn store_program(&mut self, id: ProgramId, code: Code, _init_message_id: MessageId) -> Program {
        let code_hash = code.code_hash();

        self.store_code(code_hash, code.clone());

        let program = Program::new(id, code);

        self.waiting_init.insert(program.id(), vec![]);
        self.actors.insert(
            program.id(),
            Some(ExecutableActor {
                program: program.clone(),
                balance: 0,
            }),
        );
        program
    }
    fn write_gas(&mut self, message_id: MessageId, gas_limit: u64) {
        self.gas_limits.insert(message_id, gas_limit);
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

        let dispatch_queue = dispatch_queue
            .into_iter()
            .map(|msg| {
                let id = msg.id();
                (msg, *self.gas_limits.get(&id).expect("Shouldn't fail"))
            })
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
            .position(|d| d.message().id() == message_id)
        {
            self.dispatch_queue.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, dispatch: Dispatch) {
        let destination = dispatch.destination();
        if self.actors.contains_key(&destination) || self.marked_destinations.contains(&destination)
        {
            // imbuing gas-less messages with maximum gas!
            let gas_limit = dispatch.gas_limit().unwrap_or(u64::MAX);
            self.gas_limits.insert(dispatch.id(), gas_limit);

            // Find in dispatch queue init message to the destination. By that we recognize
            // messages to not yet initialized programs, whose init messages weren't executed yet.
            let init_to_dest = self.dispatch_queue.iter().find(|d| {
                d.message().destination() == destination && d.kind() == DispatchKind::Init
            });
            if let (DispatchKind::Handle, Some(list), None) = (
                dispatch.kind(),
                self.waiting_init.get_mut(&destination),
                init_to_dest,
            ) {
                let message_id = dispatch.message().id();
                list.push(message_id);
                self.wait_list
                    .insert((destination, message_id), dispatch.into_stored());
            } else {
                self.dispatch_queue.push_back(dispatch.into_stored());
            }
        } else {
            self.log.push(dispatch.into_parts().1.into_stored());
        }
    }
    fn wait_dispatch(&mut self, dispatch: StoredDispatch) {
        self.message_consumed(dispatch.id());
        self.wait_list
            .insert((dispatch.destination(), dispatch.id()), dispatch);
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

    fn store_new_programs(&mut self, code_hash: CodeId, candidates: Vec<(ProgramId, MessageId)>) {
        if let Some(code) = self.original_codes.get(&code_hash).cloned() {
            for (candidate_id, init_message_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let code = Code::try_new(
                        code.clone(),
                        1,
                        None,
                        wasm_instrument::gas_metering::ConstantCostRules::default(),
                    )
                    .unwrap();

                    self.store_program(candidate_id, code, init_message_id);
                } else {
                    log::debug!("Program with id {} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {} for candidate programs",
                code_hash
            );
            for (invalid_candidate, _) in candidates {
                self.marked_destinations.insert(invalid_candidate);
            }
        }
    }

    fn stop_processing(&mut self, _dispatch: StoredDispatch, _gas_burned: u64) {
        panic!("Processing stopped. Used for on-chain logic only.");
    }
}
