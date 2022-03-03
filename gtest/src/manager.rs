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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) type Actor = (Program, ProgramState, u128);

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
    pub(crate) actors: BTreeMap<ProgramId, Actor>,
    pub(crate) codes: BTreeMap<CodeHash, Vec<u8>>,
    pub(crate) dispatch_queue: VecDeque<Dispatch>,
    pub(crate) mailbox: BTreeMap<ProgramId, Vec<Message>>,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), Dispatch>,
    pub(crate) wait_init_list: BTreeMap<ProgramId, Vec<MessageId>>,

    // Last run info
    pub(crate) origin: ProgramId,
    pub(crate) msg_id: MessageId,
    pub(crate) log: Vec<Message>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,

    // Additional state
    pub(crate) marked_destinations: BTreeSet<ProgramId>,
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

    pub(crate) fn store_new_program(&mut self, program_id: ProgramId, program: Program, init_message_id: Option<MessageId>) -> Option<Actor> {
        if let Program::Core(program) = &program {
            self.store_new_code(program.code());
        }
        self.actors.insert(program_id, (program, ProgramState::Uninitialized(init_message_id), 0))
    }

    pub(crate) fn store_new_code(&mut self, code: &[u8]) -> CodeHash {
        let code_hash = CodeHash::generate(code);
        self.codes.insert(code_hash, code.to_vec());
        code_hash
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

    fn entry_point(message: &Message, state: &mut ProgramState) -> DispatchKind {
        message
            .reply()
            .map(|_| DispatchKind::HandleReply)
            .unwrap_or_else(|| {
                if let ProgramState::Uninitialized(message_id) = state {
                    if matches!(message_id, Some(id) if *id != message.id()) {
                        DispatchKind::Handle
                    } else {
                        *message_id = Some(message.id());

                        DispatchKind::Init
                    }
                } else {
                    DispatchKind::Handle
                }
            })
    }

    pub(crate) fn run_message(&mut self, message: Message) -> RunResult {
        self.prepare_for(message.id(), message.source());

        logger::debug!("received message {:?}", message);

        if let Some((_, state, _)) = self.actors.get_mut(&message.dest()) {
            let dispatch = Dispatch {
                kind: Self::entry_point(&message, state),
                message,
                payload_store: None,
            };
            self.dispatch_queue.push_back(dispatch);
        } else {
            logger::debug!("received message {:?}", message);
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message);
        }

        while let Some(dispatch) = self.dispatch_queue.pop_front() {
            let Dispatch {ref message, kind, .. } = dispatch;
            let (prog, state, balance) = self
                .actors
                .get_mut(&message.dest())
                .expect("Somehow message queue contains message for user");

            let maybe_message_reply = message.reply();
            if maybe_message_reply.is_none() && matches!(state, &mut ProgramState::Uninitialized(Some(id)) if id != message.id()) {
                self.wait_init_list
                    .entry(message.dest())
                    .or_default()
                    .push(message.id());
                self.wait_dispatch(dispatch);

                continue;
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

                    logger::debug!("Executing message {:?} with kind {:?} on actor {:?}", message, kind, actor.as_ref().map(|a| (a.program.id(), a.program.is_initialized())));

                    let journal = core_processor::process::<Ext, WasmtimeEnvironment<Ext>>(
                        actor,
                        dispatch,
                        self.block_info,
                        crate::EXISTENTIAL_DEPOSIT,
                        self.origin,
                    );

                    'a: for j in &journal {
                        if let core_processor::common::JournalNote::UpdatePage{..} = j {
                            continue 'a;
                        }
                        logger::debug!("NOTE {:?}", j);
                    }

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
        let initialized_programs = self.actors
            .iter()
            .filter_map(|(&program_id, (program, state, _))| {
                let code_hash = if let Program::Core(p) = program {
                    Some(CodeHash::generate(p.code()))
                } else {
                    None
                };
                if matches!(state, &ProgramState::Initialized) {
                    Some((program_id, code_hash))
                } else {
                    None
                }
            })
            .collect();

        RunResult {
            main_failed: self.main_failed,
            others_failed: self.others_failed,
            log: log.into_iter().map(CoreLog::from_message).collect(),
            initialized_programs,
        }
    }

    fn prepare_for(&mut self, msg_id: MessageId, origin: ProgramId) {
        self.msg_id = msg_id;
        self.origin = origin;
        self.log.clear();
        self.main_failed = false;
        self.others_failed = false;

        if !self.dispatch_queue.is_empty() {
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
        let (program, state, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");
        
        if let Program::Core(p) = program {
            p.set_initialized();
        }

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
            .dispatch_queue
            .iter()
            .position(|d| d.message.id() == message_id)
        {
            self.dispatch_queue.remove(index);
        }
    }
    fn send_dispatch(&mut self, _message_id: MessageId, mut dispatch: Dispatch) {
        let Dispatch { ref mut message, .. } = dispatch;
        if self.actors.contains_key(&message.dest()) {
            // imbuing gas-less messages with maximum gas!
            if message.gas_limit.is_none() {
                message.gas_limit = Some(u64::max_value());
            }
            self.dispatch_queue.push_back(dispatch);
        } else {
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message.clone());
            self.log.push(dispatch.message);
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.message_consumed(dispatch.message.id());
        self.wait_list.insert(
            (dispatch.message.dest(), dispatch.message.id()),
            dispatch,
        );
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
        code_hash: CodeHash,
        candidates: Vec<(ProgramId, MessageId)>,
    ) {
        if let Some(code) = self.codes.get(&code_hash).cloned() {
            for (candidate_id, init_message_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let program = CoreProgram::new(candidate_id, code.clone())
                        .expect(format!("internal error: program can't be constructed with provided code {:?}", code).as_str());
                    self.store_new_program(candidate_id, Program::Core(program), Some(init_message_id));
                } else {
                    logger::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            logger::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_hash
            );
            for (invalid_candidate, _) in candidates {
                self.marked_destinations.insert(invalid_candidate);
            }
        }
    }
}
