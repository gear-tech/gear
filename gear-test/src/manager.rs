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
    code::{Code, CodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, PageBuf, PageU32Size, WasmPage},
    message::{Dispatch, DispatchKind, GasLimit, MessageWaitedType, StoredDispatch, StoredMessage},
    reservation::GasReserver,
};
use gear_core_errors::SimpleSignalError;
use gear_wasm_instrument::wasm_instrument::gas_metering::ConstantCostRules;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

#[derive(Clone, Default)]
/// In-memory state.
pub struct State {
    /// Message queue.
    pub dispatch_queue: VecDeque<(StoredDispatch, GasLimit)>,
    /// Log records.
    pub log: Vec<StoredMessage>,
    /// State of each actor.
    pub actors: BTreeMap<ProgramId, TestActor>,
    /// Is current state failed.
    pub current_failed: bool,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("dispatch_queue", &self.dispatch_queue)
            .field("log", &self.log)
            .field(
                "actors",
                &self
                    .actors
                    .iter()
                    .filter_map(|(id, actor)| {
                        actor
                            .executable_data
                            .as_ref()
                            .map(|data| (*id, (actor.balance, &data.allocations)))
                    })
                    .collect::<BTreeMap<ProgramId, (u128, &BTreeSet<WasmPage>)>>(),
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

#[derive(Clone)]
pub struct TestActor {
    pub balance: u128,
    pub executable_data: Option<ExecutableActorData>,
    pub memory_pages: BTreeMap<GearPage, PageBuf>,
}

impl TestActor {
    pub fn into_parts(self, dest: ProgramId) -> (Actor, BTreeMap<GearPage, PageBuf>) {
        let Self {
            balance,
            executable_data,
            memory_pages,
        } = self;

        (
            Actor {
                balance,
                destination_program: dest,
                executable_data,
            },
            memory_pages,
        )
    }
}

#[derive(Clone, Default)]
pub struct InMemoryExtManager {
    original_codes: BTreeMap<CodeId, Vec<u8>>,
    codes: BTreeMap<CodeId, Code>,
    marked_destinations: BTreeSet<ProgramId>,
    dispatch_queue: VecDeque<StoredDispatch>,
    log: Vec<StoredMessage>,
    actors: BTreeMap<ProgramId, TestActor>,
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
    fn store_code(&mut self, code_id: CodeId, code: Code) {
        self.codes.insert(code_id, code);
    }

    fn load_code(&self, code_id: CodeId) -> Option<Code> {
        self.codes.get(&code_id).cloned()
    }

    fn store_original_code(&mut self, code: &[u8]) {
        self.original_codes
            .insert(CodeId::generate(code), code.to_vec());
    }
    fn store_program(
        &mut self,
        id: ProgramId,
        code: Code,
        _init_message_id: MessageId,
    ) -> ExecutableActorData {
        let code_and_id = CodeAndId::new(code);
        let code_id = code_and_id.code_id();

        self.store_code(code_and_id.code_id(), code_and_id.code().clone());

        let actor_data = ExecutableActorData {
            allocations: Default::default(),
            code_id,
            code_exports: code_and_id.code().exports().clone(),
            static_pages: code_and_id.code().static_pages(),
            initialized: false,
            pages_with_data: Default::default(),
            gas_reservation_map: Default::default(),
        };

        self.waiting_init.insert(id, vec![]);
        self.actors.insert(
            id,
            TestActor {
                balance: 0,
                executable_data: Some(actor_data.clone()),
                memory_pages: Default::default(),
            },
        );

        actor_data
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
    fn message_dispatched(
        &mut self,
        _message_id: MessageId,
        _source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        self.current_failed = match outcome {
            DispatchOutcome::MessageTrap { .. } => true,
            DispatchOutcome::InitFailure { program_id, .. } => {
                self.move_waiting_msgs_to_queue(program_id);
                if let Some(actor) = self.actors.get_mut(&program_id) {
                    // Program is now considered terminated (in opposite to active). But not deleted from the state.
                    actor.executable_data = None;
                }
                true
            }
            DispatchOutcome::Success
            | DispatchOutcome::NoExecution
            | DispatchOutcome::Exit { .. } => false,
            DispatchOutcome::InitSuccess { program_id, .. } => {
                if let Some(TestActor {
                    executable_data: Some(data),
                    ..
                }) = self.actors.get_mut(&program_id)
                {
                    data.initialized = true;
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
    fn send_dispatch(
        &mut self,
        _message_id: MessageId,
        dispatch: Dispatch,
        _delay: u32,
        _reservation: Option<ReservationId>,
    ) {
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
    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        _duration: Option<u32>,
        _: MessageWaitedType,
    ) {
        self.message_consumed(dispatch.id());
        self.wait_list
            .insert((dispatch.destination(), dispatch.id()), dispatch);
    }
    fn wake_message(
        &mut self,
        _message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        _delay: u32,
    ) {
        if let Some(dispatch) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.dispatch_queue.push_back(dispatch);
        }
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        mut pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        if let TestActor {
            executable_data: Some(_),
            memory_pages,
            ..
        } = self
            .actors
            .get_mut(&program_id)
            .expect("Program not found in storage")
        {
            memory_pages.append(&mut pages_data);
        } else {
            unreachable!("Can't update page for terminated program");
        }
    }

    fn update_allocations(&mut self, program_id: ProgramId, allocations: BTreeSet<WasmPage>) {
        if let TestActor {
            executable_data: Some(data),
            memory_pages,
            ..
        } = self
            .actors
            .get_mut(&program_id)
            .expect("Program not found in storage")
        {
            for page in data
                .allocations
                .difference(&allocations)
                .flat_map(|page| page.to_pages_iter())
            {
                memory_pages.remove(&page);
            }

            data.allocations = allocations;
        } else {
            unreachable!("Can't update allocations for terminated program");
        }
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        if let Some(to) = to {
            if let Some(actor) = self.actors.get_mut(&from) {
                if actor.balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                actor.balance -= value;
            };

            if let Some(actor) = self.actors.get_mut(&to) {
                actor.balance += value;
            };
        };
    }

    fn store_new_programs(&mut self, code_id: CodeId, candidates: Vec<(MessageId, ProgramId)>) {
        if let Some(code) = self.original_codes.get(&code_id).cloned() {
            for (init_message_id, candidate_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let code =
                        Code::try_new(code.clone(), 1, |_| ConstantCostRules::default(), None)
                            .unwrap();

                    self.store_program(candidate_id, code, init_message_id);
                } else {
                    log::debug!("Program with id {} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {} for candidate programs",
                code_id
            );
            for (_, invalid_candidate) in candidates {
                self.marked_destinations.insert(invalid_candidate);
            }
        }
    }

    fn stop_processing(&mut self, _dispatch: StoredDispatch, _gas_burned: u64) {
        panic!("Processing stopped. Used for on-chain logic only.");
    }

    fn reserve_gas(
        &mut self,
        _message_id: MessageId,
        _reservation_id: ReservationId,
        _program_id: ProgramId,
        _amount: u64,
        _bn: u32,
    ) {
    }

    fn unreserve_gas(
        &mut self,
        _reservation_id: ReservationId,
        _program_id: ProgramId,
        _expiration: u32,
    ) {
    }

    fn update_gas_reservation(&mut self, program_id: ProgramId, reserver: GasReserver) {
        let actor = self
            .actors
            .get_mut(&program_id)
            .expect("gas reservation update guaranteed to be called only on existing program");

        if let TestActor {
            executable_data: Some(executable_data),
            ..
        } = actor
        {
            executable_data.gas_reservation_map = reserver.into_map(1, |duration| duration);
        } else {
            panic!("no gas reservation map found in program");
        }
    }

    fn system_reserve_gas(&mut self, _message_id: MessageId, _amount: u64) {}

    fn system_unreserve_gas(&mut self, _message_id: MessageId) {}

    fn send_signal(
        &mut self,
        _message_id: MessageId,
        _destination: ProgramId,
        _err: SimpleSignalError,
    ) {
    }
}
