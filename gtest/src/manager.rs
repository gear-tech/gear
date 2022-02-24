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
    memory::PageNumber,
    message::{Dispatch, DispatchKind, Message, MessageId},
    program::{CodeHash, Program as CoreProgram, ProgramId},
};
use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
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
    pub(crate) message_queue: VecDeque<Message>,
    pub(crate) mailbox: BTreeMap<ProgramId, Vec<Message>>,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), Message>,
    pub(crate) wait_init_list: BTreeMap<ProgramId, Vec<MessageId>>,

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

    fn entry_point(message: &Message, state: &mut ProgramState) -> Option<DispatchKind> {
        message
            .reply()
            .map(|_| DispatchKind::HandleReply)
            .or_else(|| {
                if let ProgramState::Uninitialized(message_id) = state {
                    if let Some(id) = message_id {
                        if *id == message.id() {
                            Some(DispatchKind::Init)
                        } else {
                            None
                        }
                    } else {
                        *message_id = Some(message.id());

                        Some(DispatchKind::Init)
                    }
                } else {
                    Some(DispatchKind::Handle)
                }
            })
    }

    pub(crate) fn run_message(&mut self, message: Message) -> RunResult {
        self.prepare_for(message.id(), message.source());

        if self.actors.contains_key(&message.dest()) {
            self.message_queue.push_back(message);
        } else {
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message);
        }

        while let Some(message) = self.message_queue.pop_front() {
            let (prog, state, balance) = self
                .actors
                .get_mut(&message.dest())
                .expect("Somehow message queue contains message for user");

            let kind = if let Some(kind) = Self::entry_point(&message, state) {
                kind
            } else {
                self.wait_init_list
                    .entry(message.dest())
                    .or_default()
                    .push(message.id());

                self.wait_dispatch(Dispatch {
                    kind: DispatchKind::Handle,
                    message,
                    payload_store: None,
                });

                continue;
            };

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

                    let journal = core_processor::process::<Ext, WasmtimeEnvironment<Ext>>(
                        actor,
                        Dispatch {
                            kind,
                            message,
                            payload_store: None,
                        },
                        self.block_info,
                        crate::EXISTENTIAL_DEPOSIT,
                        self.origin,
                    );

                    core_processor::handle_journal(journal, self);
                }
                Program::Mock(mock) => {
                    let payload = message.payload().to_vec();

                    let response = match kind {
                        DispatchKind::Init => mock.init(payload),
                        DispatchKind::Handle => mock.handle(payload),
                        DispatchKind::HandleReply => mock.handle_reply(payload),
                    };

                    let message_id = message.id();
                    let program_id = message.dest();

                    match response {
                        Ok(reply) => {
                            if let DispatchKind::Init = kind {
                                self.init_success(message_id, program_id)
                            }

                            if let Some(payload) = reply {
                                let nonce = self.fetch_inc_message_nonce();

                                let reply_message = Message::new_reply(
                                    nonce.into(),
                                    program_id,
                                    message.source(),
                                    payload.into(),
                                    0,
                                    message.id(),
                                    0,
                                );
                                self.send_dispatch(message_id, Dispatch::new_reply(reply_message));
                            }
                        }
                        Err(expl) => {
                            mock.debug(expl);

                            if let DispatchKind::Init = kind {
                                self.message_dispatched(DispatchOutcome::InitFailure {
                                    message_id,
                                    program_id,
                                    origin: message.source(),
                                    reason: expl,
                                });
                            } else {
                                self.message_dispatched(DispatchOutcome::MessageTrap {
                                    message_id,
                                    program_id,
                                    trap: Some(expl),
                                })
                            }

                            let nonce = self.fetch_inc_message_nonce();

                            let reply_message = Message::new_reply(
                                nonce.into(),
                                program_id,
                                message.source(),
                                Default::default(),
                                0,
                                message.id(),
                                1,
                            );
                            self.send_dispatch(message_id, Dispatch::new_reply(reply_message));
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

    fn prepare_for(&mut self, msg_id: MessageId, origin: ProgramId) {
        self.msg_id = msg_id;
        self.origin = origin;
        self.log.clear();
        self.main_failed = false;
        self.others_failed = false;

        if !self.message_queue.is_empty() {
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
            .message_queue
            .iter()
            .position(|msg| msg.id() == message_id)
        {
            self.message_queue.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, dispatch: Dispatch) {
        let Dispatch { mut message, .. } = dispatch;
        if self.actors.contains_key(&message.dest()) {
            // imbuing gas-less messages with maximum gas!
            if message.gas_limit.is_none() {
                message.gas_limit = Some(u64::max_value());
            }
            self.message_queue.push_back(message);
        } else {
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message.clone());
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
        if let Some((Program::Core(prog), ..)) = self.actors.get_mut(&program_id) {
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

    fn store_new_programs(
        &mut self,
        _code_hash: CodeHash,
        _candidates: Vec<(ProgramId, MessageId)>,
    ) {
        // todo!() #714
    }
}
