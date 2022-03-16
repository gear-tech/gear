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

use crate::{
    log::{CoreLog, RunResult},
    program::WasmProgram,
};
use core_processor::{common::*, configs::BlockInfo, Ext};
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::{
    identifiers::{CodeId, MessageId, ProgramId},
    memory::PageNumber,
    message::{
        Dispatch, DispatchKind, Message, ReplyMessage, ReplyPacket, StoredDispatch, StoredMessage,
    },
    program::Program as CoreProgram,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProgramState {
    Initialized,
    Uninitialized(Option<MessageId>),
    FailedInitialization,
}

#[derive(Debug)]
pub(crate) enum Program {
    Core(CoreProgram),
    Mock(Box<dyn WasmProgram>),
}

#[derive(Default, Debug)]
pub(crate) struct ExtManager {
    // State metadata
    pub(crate) block_info: BlockInfo,

    // Messaging and programs meta
    pub(crate) msg_nonce: u64,
    pub(crate) id_nonce: u64,

    // State
    pub(crate) actors: BTreeMap<ProgramId, (Program, ProgramState, u128)>,
    pub(crate) dispatches: VecDeque<StoredDispatch>,
    pub(crate) mailbox: BTreeMap<ProgramId, Vec<StoredMessage>>,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), StoredDispatch>,
    pub(crate) wait_init_list: BTreeMap<ProgramId, Vec<MessageId>>,
    pub(crate) gas_limits: BTreeMap<MessageId, Option<u64>>,

    // Last run info
    pub(crate) origin: ProgramId,
    pub(crate) msg_id: MessageId,
    pub(crate) log: Vec<Message>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,
}

impl ExtManager {
    pub(crate) fn new() -> Self {
        Self {
            msg_nonce: 1,
            id_nonce: 1,
            block_info: BlockInfo {
                height: 0,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_secs(),
            },
            ..Default::default()
        }
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        while self.actors.contains_key(&self.id_nonce.into()) {
            self.id_nonce += 1;
        }
        self.id_nonce
    }

    pub(crate) fn run_dispatch(&mut self, dispatch: Dispatch) -> RunResult {
        self.prepare_for(&dispatch);

        self.gas_limits.insert(dispatch.id(), dispatch.gas_limit());
        let dispatch = dispatch.into_stored();

        if self.actors.contains_key(&dispatch.destination()) {
            self.dispatches.push_back(dispatch);
        } else {
            self.mailbox
                .entry(dispatch.destination())
                .or_default()
                .push(dispatch.message().clone());
        }

        while let Some(dispatch) = self.dispatches.pop_front() {
            let (prog, state, balance) = self
                .actors
                .get_mut(&dispatch.destination())
                .expect("Somehow message queue contains message for user");

            if *state == ProgramState::Initialized && dispatch.kind() == DispatchKind::Init {
                panic!("Double initialization");
            }

            if let ProgramState::Uninitialized(id) = state {
                match dispatch.kind() {
                    DispatchKind::Init => {
                        *id = Some(dispatch.id());
                    }
                    DispatchKind::Handle => {
                        self.wait_init_list
                            .entry(dispatch.destination())
                            .or_default()
                            .push(dispatch.id());

                        self.wait_dispatch(dispatch);

                        continue;
                    }
                    _ => {}
                }
            }

            match prog {
                Program::Core(program) => {
                    let actor = if let ProgramState::FailedInitialization = state {
                        None
                    } else {
                        Some(ExecutableActor {
                            program: program.clone(),
                            balance: *balance,
                        })
                    };

                    let gas_limit = self
                        .gas_limits
                        .get(&dispatch.id())
                        .expect("Unable to find associated gas limit")
                        .unwrap_or(u64::MAX);
                    let dispatch = dispatch.into_incoming(gas_limit);

                    let journal = core_processor::process::<Ext, WasmtimeEnvironment<Ext>>(
                        actor,
                        dispatch,
                        self.block_info,
                        crate::EXISTENTIAL_DEPOSIT,
                        self.origin,
                        program.id(),
                    );

                    core_processor::handle_journal(journal, self);
                }
                Program::Mock(mock) => {
                    let payload = dispatch.payload().to_vec();

                    let response = match dispatch.kind() {
                        DispatchKind::Init => mock.init(payload),
                        DispatchKind::Handle => mock.handle(payload),
                        DispatchKind::Reply => mock.handle_reply(payload),
                    };

                    let message_id = dispatch.id();
                    let program_id = dispatch.destination();

                    match response {
                        Ok(reply) => {
                            if let DispatchKind::Init = dispatch.kind() {
                                self.init_success(message_id, program_id)
                            }

                            if let Some(payload) = reply {
                                let id = MessageId::generate_reply(dispatch.id(), 0);
                                let packet = ReplyPacket::new(payload, 0);
                                let message = ReplyMessage::from_packet(id, packet);

                                self.send_dispatch(
                                    message_id,
                                    message.into_dispatch(
                                        dispatch.destination(),
                                        dispatch.source(),
                                        dispatch.id(),
                                    ),
                                );
                            }
                        }
                        Err(expl) => {
                            mock.debug(expl);

                            if let DispatchKind::Init = dispatch.kind() {
                                self.message_dispatched(DispatchOutcome::InitFailure {
                                    message_id,
                                    program_id,
                                    origin: dispatch.source(),
                                    reason: expl,
                                });
                            } else {
                                self.message_dispatched(DispatchOutcome::MessageTrap {
                                    message_id,
                                    program_id,
                                    trap: Some(expl),
                                })
                            }

                            let id = MessageId::generate_reply(dispatch.id(), 1);
                            let packet = ReplyPacket::system(1);
                            let message = ReplyMessage::from_packet(id, packet);

                            self.send_dispatch(
                                message_id,
                                message.into_dispatch(
                                    dispatch.destination(),
                                    dispatch.source(),
                                    dispatch.id(),
                                ),
                            );
                        }
                    }
                }
            }
        }

        let log = self.log.clone();

        RunResult {
            main_failed: self.main_failed,
            others_failed: self.others_failed,
            log: log.into_iter().map(CoreLog::from_message).collect(),
        }
    }

    fn prepare_for(&mut self, dispatch: &Dispatch) {
        self.msg_id = dispatch.id();
        self.origin = dispatch.source();
        self.log.clear();
        self.main_failed = false;
        self.others_failed = false;

        // TODO: Remove this check after #349.
        if !self.dispatches.is_empty() {
            panic!("Message queue isn't empty");
        }
    }

    fn mark_failed(&mut self, msg_id: MessageId) {
        if self.msg_id == msg_id {
            self.main_failed = true;
        } else {
            self.others_failed = true;
        }
    }

    fn init_success(&mut self, message_id: MessageId, program_id: ProgramId) {
        let (_, state, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        *state = ProgramState::Initialized;

        self.move_waiting_msgs_to_queue(message_id, program_id);
    }

    fn init_failure(&mut self, message_id: MessageId, program_id: ProgramId) {
        let (_, state, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        *state = ProgramState::FailedInitialization;

        self.move_waiting_msgs_to_queue(message_id, program_id);
        self.mark_failed(message_id);
    }

    fn move_waiting_msgs_to_queue(&mut self, message_id: MessageId, program_id: ProgramId) {
        if let Some(ids) = self.wait_init_list.remove(&program_id) {
            for id in ids {
                self.wake_message(message_id, program_id, id);
            }
        }
    }
}

impl JournalHandler for ExtManager {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        match outcome {
            DispatchOutcome::MessageTrap { message_id, .. } => self.mark_failed(message_id),
            DispatchOutcome::Success(_) | DispatchOutcome::NoExecution(_) => {}
            DispatchOutcome::InitFailure {
                message_id,
                program_id,
                ..
            } => self.init_failure(message_id, program_id),
            DispatchOutcome::InitSuccess {
                message_id,
                program_id,
                ..
            } => self.init_success(message_id, program_id),
        }
    }
    fn gas_burned(&mut self, _message_id: MessageId, _amount: u64) {}

    fn exit_dispatch(&mut self, id_exited: ProgramId, _value_destination: ProgramId) {
        self.actors.remove(&id_exited);
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        if let Some(index) = self
            .dispatches
            .iter()
            .position(|msg| msg.id() == message_id)
        {
            self.dispatches.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, dispatch: Dispatch) {
        self.gas_limits.insert(dispatch.id(), dispatch.gas_limit());

        if self.actors.contains_key(&dispatch.destination()) {
            self.dispatches.push_back(dispatch.into_stored());
        } else {
            self.mailbox
                .entry(dispatch.destination())
                .or_default()
                .push(dispatch.message().clone().into_stored());
            self.log.push(dispatch.message().clone());
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
        if let Some(msg) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.dispatches.push_back(msg);
        }
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        if let Some((Program::Core(prog), ..)) = self.actors.get_mut(&program_id) {
            if let Some(data) = data {
                let _ = prog.set_page(page_number, &data);
            } else {
                prog.remove_page(page_number);
            }
        } else {
            panic!("Program not found in storage");
        }
    }
    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        if let Some(to) = to {
            if let Some((.., balance)) = self.actors.get_mut(&from) {
                if *balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                *balance -= value;
            };

            if let Some((.., balance)) = self.actors.get_mut(&to) {
                *balance += value;
            };
        }
    }

    fn store_new_programs(&mut self, _code_hash: CodeId, _candidates: Vec<(ProgramId, MessageId)>) {
        // todo!() #714
    }
}
