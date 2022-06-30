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
    program::{Gas, WasmProgram},
    wasm_executor::WasmExecutor,
    Result, TestError,
};
use core_processor::{common::*, configs::BlockInfo, Ext};
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::{
        Dispatch, DispatchKind, Payload, ReplyMessage, ReplyPacket, StoredDispatch, StoredMessage,
    },
    program::Program as CoreProgram,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    time::{SystemTime, UNIX_EPOCH},
};
use wasm_instrument::gas_metering::ConstantCostRules;

const OUTGOING_LIMIT: u32 = 1024;

pub(crate) type Balance = u128;

#[derive(Debug)]
pub(crate) enum Actor {
    Initialized(Program),
    // Contract: program is always `Some`, option is used to take ownership
    Uninitialized(Option<MessageId>, Option<Program>),
    Dormant,
    User,
}

impl Actor {
    fn new(init_message_id: Option<MessageId>, program: Program) -> Self {
        Actor::Uninitialized(init_message_id, Some(program))
    }

    // # Panics
    // If actor is initialized or dormant
    fn set_initialized(&mut self) {
        assert!(
            self.is_uninitialized(),
            "can't transmute actor, which isn't uninitialized"
        );

        if let Actor::Uninitialized(_, maybe_prog) = self {
            let mut prog = maybe_prog
                .take()
                .expect("actor storage contains only `Some` values by contract");
            if let Program::Genuine { program, .. } = &mut prog {
                program.set_initialized();
            }
            *self = Actor::Initialized(prog);
        }
    }

    fn is_dormant(&self) -> bool {
        matches!(self, Actor::Dormant)
    }

    fn is_uninitialized(&self) -> bool {
        matches!(self, Actor::Uninitialized(..))
    }

    fn get_pages_data_mut(&mut self) -> Option<&mut BTreeMap<PageNumber, PageBuf>> {
        match self {
            Actor::Initialized(Program::Genuine { pages_data, .. }) => Some(pages_data),
            _ => None,
        }
    }

    // Takes ownership over mock program, putting `None` value instead of it.
    fn take_mock(&mut self) -> Option<Box<dyn WasmProgram>> {
        match self {
            Actor::Initialized(Program::Mock(mock))
            | Actor::Uninitialized(_, Some(Program::Mock(mock))) => mock.take(),
            _ => None,
        }
    }

    fn code_id(&self) -> Option<CodeId> {
        match self {
            Actor::Initialized(Program::Genuine { code_id, .. })
            | Actor::Uninitialized(_, Some(Program::Genuine { code_id, .. })) => Some(*code_id),
            _ => None,
        }
    }

    // Gets a new executable actor derived from the inner program.
    fn get_executable_actor(&self, balance: Balance) -> Option<ExecutableActor> {
        let (program, pages_data) = match self {
            Actor::Initialized(Program::Genuine {
                program,
                pages_data,
                ..
            })
            | Actor::Uninitialized(
                _,
                Some(Program::Genuine {
                    program,
                    pages_data,
                    ..
                }),
            ) => (program.clone(), pages_data.clone()),
            _ => return None,
        };
        Some(ExecutableActor {
            program,
            balance,
            pages_data,
        })
    }
}

#[derive(Debug)]
pub(crate) enum Program {
    Genuine {
        program: CoreProgram,
        code_id: CodeId,
        pages_data: BTreeMap<PageNumber, PageBuf>,
    },
    // Contract: is always `Some`, option is used to take ownership
    Mock(Option<Box<dyn WasmProgram>>),
}

impl Program {
    pub(crate) fn new(
        program: CoreProgram,
        code_id: CodeId,
        pages_data: BTreeMap<PageNumber, PageBuf>,
    ) -> Self {
        Program::Genuine {
            program,
            code_id,
            pages_data,
        }
    }

    pub(crate) fn new_mock(mock: impl WasmProgram + 'static) -> Self {
        Program::Mock(Some(Box::new(mock)))
    }
}

#[derive(Default, Debug)]
pub(crate) struct ExtManager {
    // State metadata
    pub(crate) block_info: BlockInfo,

    // Messaging and programs meta
    pub(crate) msg_nonce: u64,
    pub(crate) id_nonce: u64,

    // State
    pub(crate) actors: BTreeMap<ProgramId, (Actor, Balance)>,
    pub(crate) opt_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) meta_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) dispatches: VecDeque<StoredDispatch>,
    pub(crate) mailbox: HashMap<ProgramId, Vec<StoredMessage>>,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), StoredDispatch>,
    pub(crate) wait_init_list: BTreeMap<ProgramId, Vec<MessageId>>,
    pub(crate) gas_limits: BTreeMap<MessageId, Option<u64>>,

    // Last run info
    pub(crate) origin: ProgramId,
    pub(crate) msg_id: MessageId,
    pub(crate) log: Vec<StoredMessage>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,
    pub(crate) main_gas_burned: Gas,
    pub(crate) others_gas_burned: Gas,
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

    pub(crate) fn store_new_actor(
        &mut self,
        program_id: ProgramId,
        program: Program,
        init_message_id: Option<MessageId>,
    ) -> Option<(Actor, Balance)> {
        if let Program::Genuine { program, .. } = &program {
            self.store_new_code(program.raw_code());
        }
        self.actors
            .insert(program_id, (Actor::new(init_message_id, program), 0))
    }

    pub(crate) fn store_new_code(&mut self, code: &[u8]) -> CodeId {
        let code_hash = CodeId::generate(code);
        self.opt_binaries.insert(code_hash, code.to_vec());
        code_hash
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        while self.actors.contains_key(&self.id_nonce.into())
            || self.mailbox.contains_key(&self.id_nonce.into())
        {
            self.id_nonce += 1;
        }
        self.id_nonce
    }

    pub(crate) fn run_dispatch(&mut self, dispatch: Dispatch) -> RunResult {
        self.prepare_for(&dispatch);

        self.gas_limits.insert(dispatch.id(), dispatch.gas_limit());

        if !self.is_user(&dispatch.destination()) {
            self.dispatches.push_back(dispatch.into_stored());
        } else {
            let message = dispatch.into_parts().1.into_stored();

            self.mailbox
                .entry(message.destination())
                .or_default()
                .push(message.clone());

            self.log.push(message)
        }

        let mut total_processed = 0;
        while let Some(dispatch) = self.dispatches.pop_front() {
            let message_id = dispatch.id();
            let dest = dispatch.destination();

            if self.check_is_for_wait_list(&dispatch) {
                self.wait_init_list
                    .entry(dest)
                    .or_default()
                    .push(message_id);
                self.wait_dispatch(dispatch);

                continue;
            }

            let (actor, balance) = self
                .actors
                .get_mut(&dest)
                .expect("Somehow message queue contains message for user");

            if actor.is_dormant() {
                self.process_dormant(dispatch);
            } else if let Some(executable_actor) = actor.get_executable_actor(*balance) {
                self.process_normal(executable_actor, dispatch);
            } else if let Some(mock) = actor.take_mock() {
                self.process_mock(mock, dispatch);
            } else {
                unreachable!();
            }

            total_processed += 1;
        }

        let log = self.log.clone();

        RunResult {
            main_failed: self.main_failed,
            others_failed: self.others_failed,
            log: log.into_iter().map(CoreLog::from).collect(),
            message_id: self.msg_id,
            total_processed,
            main_gas_burned: self.main_gas_burned,
            others_gas_burned: self.others_gas_burned,
        }
    }

    /// Call non-void meta function from actor stored in manager.
    /// Warning! This is a static call that doesn't change actors pages data.
    pub(crate) fn call_meta(
        &mut self,
        program_id: &ProgramId,
        payload: Option<Payload>,
        function_name: &str,
    ) -> Result<Vec<u8>> {
        let mut executor = self.get_executor(program_id, payload)?;
        executor.execute(function_name)
    }

    pub(crate) fn is_user(&self, id: &ProgramId) -> bool {
        !self.actors.contains_key(id) || matches!(self.actors.get(id), Some((Actor::User, _)))
    }

    pub(crate) fn add_value_to(&mut self, id: &ProgramId, value: Balance) {
        let (_, balance) = self.actors.entry(*id).or_insert((Actor::User, 0));
        *balance = balance.saturating_add(value);
    }

    pub(crate) fn actor_balance(&self, id: &ProgramId) -> Option<Balance> {
        self.actors.get(id).map(|(_, balance)| *balance)
    }

    pub(crate) fn claim_value_from_mailbox(&mut self, id: &ProgramId) {
        let messages = self.mailbox.remove(id);
        if let Some(messages) = messages {
            messages.iter().for_each(|message| {
                self.send_value(
                    message.source(),
                    Some(message.destination()),
                    message.value(),
                )
            });
        }
    }

    fn prepare_for(&mut self, dispatch: &Dispatch) {
        self.msg_id = dispatch.id();
        self.origin = dispatch.source();
        self.log.clear();
        self.main_failed = false;
        self.others_failed = false;
        self.main_gas_burned = Gas::zero();
        self.others_gas_burned = Gas::zero();

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
        let (actor, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        actor.set_initialized();

        self.move_waiting_msgs_to_queue(message_id, program_id);
    }

    fn init_failure(&mut self, message_id: MessageId, program_id: ProgramId) {
        let (actor, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        *actor = Actor::Dormant;

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

    // When called for the `dispatch`, it must be in queue.
    fn check_is_for_wait_list(&self, dispatch: &StoredDispatch) -> bool {
        let (actor, _) = self
            .actors
            .get(&dispatch.destination())
            .expect("method called for unknown destination");
        if let Actor::Uninitialized(maybe_message_id, _) = actor {
            let id = maybe_message_id.expect("message in dispatch queue has id");
            dispatch.reply().is_none() && id != dispatch.id()
        } else {
            false
        }
    }

    fn process_mock(&mut self, mut mock: Box<dyn WasmProgram>, dispatch: StoredDispatch) {
        let message_id = dispatch.id();
        let source = dispatch.source();
        let program_id = dispatch.destination();
        let payload = dispatch.payload().to_vec();

        let response = match dispatch.kind() {
            DispatchKind::Init => mock.init(payload),
            DispatchKind::Handle => mock.handle(payload),
            DispatchKind::Reply => mock.handle_reply(payload),
        };

        match response {
            Ok(reply) => {
                if let DispatchKind::Init = dispatch.kind() {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::InitSuccess { program_id },
                    );
                }

                if let Some(payload) = reply {
                    let id = MessageId::generate_reply(message_id, 0);
                    let packet = ReplyPacket::new(payload, 0);
                    let reply_message = ReplyMessage::from_packet(id, packet);

                    self.send_dispatch(
                        message_id,
                        reply_message.into_dispatch(program_id, dispatch.source(), message_id),
                    );
                }
            }
            Err(expl) => {
                mock.debug(expl);

                if let DispatchKind::Init = dispatch.kind() {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::InitFailure {
                            program_id,
                            reason: expl.to_string(),
                        },
                    );
                } else {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::MessageTrap {
                            program_id,
                            trap: expl.to_string(),
                        },
                    )
                }

                let id = MessageId::generate_reply(message_id, 1);
                let packet = ReplyPacket::new(Default::default(), 1);
                let reply_message = ReplyMessage::from_packet(id, packet);

                self.send_dispatch(
                    message_id,
                    reply_message.into_dispatch(program_id, dispatch.source(), message_id),
                );
            }
        }

        // After run either `init_success` is called or `init_failed`.
        // So only active (init success) program can be modified
        self.actors.entry(program_id).and_modify(|(actor, _)| {
            if let Actor::Initialized(old_mock) = actor {
                *old_mock = Program::Mock(Some(mock));
            }
        });
    }

    fn process_normal(&mut self, executable_actor: ExecutableActor, dispatch: StoredDispatch) {
        self.process_dispatch(Some(executable_actor), dispatch);
    }

    fn process_dormant(&mut self, dispatch: StoredDispatch) {
        self.process_dispatch(None, dispatch);
    }

    fn process_dispatch(
        &mut self,
        executable_actor: Option<ExecutableActor>,
        dispatch: StoredDispatch,
    ) {
        let dest = dispatch.destination();
        let gas_limit = self
            .gas_limits
            .get(&dispatch.id())
            .expect("Unable to find gas limit for message")
            .unwrap_or(u64::MAX);
        let journal = core_processor::process::<Ext, WasmtimeEnvironment<Ext>>(
            executable_actor,
            dispatch.into_incoming(gas_limit),
            self.block_info,
            Default::default(),
            crate::EXISTENTIAL_DEPOSIT,
            self.origin,
            dest,
            u64::MAX,
            OUTGOING_LIMIT,
            Default::default(),
            Default::default(),
            crate::MAILBOX_THRESHOLD,
        );

        core_processor::handle_journal(journal, self);
    }

    fn get_executor(
        &mut self,
        program_id: &ProgramId,
        payload: Option<Payload>,
    ) -> Result<WasmExecutor> {
        let (actor, balance) = self
            .actors
            .get_mut(program_id)
            .ok_or(TestError::ActorNotFound(*program_id))?;

        let code_id = actor.code_id();
        let actor = actor
            .get_executable_actor(*balance)
            .ok_or(TestError::ActorIsntExecutable(*program_id))?;
        let pages_initial_data = actor
            .pages_data
            .into_iter()
            .map(|(page, data)| (page, Box::new(data)))
            .collect();
        let meta_binary = code_id
            .and_then(|code_id| self.meta_binaries.get(&code_id))
            .map(Vec::as_slice)
            .ok_or(TestError::MetaBinaryNotProvided)?;

        WasmExecutor::new(&actor.program, meta_binary, &pages_initial_data, payload)
    }
}

impl JournalHandler for ExtManager {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        _source: ProgramId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::MessageTrap { .. } => self.mark_failed(message_id),
            DispatchOutcome::Success
            | DispatchOutcome::NoExecution
            | DispatchOutcome::Exit { .. } => {}
            DispatchOutcome::InitFailure { program_id, .. } => {
                self.init_failure(message_id, program_id)
            }
            DispatchOutcome::InitSuccess { program_id, .. } => {
                self.init_success(message_id, program_id)
            }
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        if self.msg_id == message_id {
            self.main_gas_burned = self.main_gas_burned.saturating_add(Gas(amount));
        } else {
            self.others_gas_burned = self.others_gas_burned.saturating_add(Gas(amount));
        }
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        if let Some((_, balance)) = self.actors.remove(&id_exited) {
            self.add_value_to(&value_destination, balance);
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        if let Some(index) = self.dispatches.iter().position(|d| d.id() == message_id) {
            self.dispatches.remove(index);
        }
    }

    fn send_dispatch(&mut self, _message_id: MessageId, dispatch: Dispatch) {
        self.gas_limits.insert(dispatch.id(), dispatch.gas_limit());

        if !self.is_user(&dispatch.destination()) {
            self.dispatches.push_back(dispatch.into_stored());
        } else {
            let message = dispatch.into_parts().1.into_stored();

            self.mailbox
                .entry(message.destination())
                .or_default()
                .push(message.clone());

            self.log.push(message);
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

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        mut pages_data: BTreeMap<PageNumber, PageBuf>,
    ) {
        let (actor, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");
        if let Some(actor_pages_data) = actor.get_pages_data_mut() {
            actor_pages_data.append(&mut pages_data);
        } else {
            unreachable!("No pages update for non-initialized program")
        }
    }

    fn update_allocations(&mut self, program_id: ProgramId, allocations: BTreeSet<WasmPageNumber>) {
        let (actor, _) = self
            .actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        if let Actor::Initialized(Program::Genuine {
            program,
            pages_data,
            ..
        }) = actor
        {
            for page in program
                .get_allocations()
                .difference(&allocations)
                .flat_map(|p| p.to_gear_pages_iter())
            {
                pages_data.remove(&page);
            }
            *program.get_allocations_mut() = allocations;
        } else {
            unreachable!("No pages update for non-initialized program")
        }
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: Balance) {
        if let Some(ref to) = to {
            if let Some((_, balance)) = self.actors.get_mut(&from) {
                if *balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                *balance -= value;
            };

            self.add_value_to(to, value);
        }
    }

    fn store_new_programs(&mut self, code_hash: CodeId, candidates: Vec<(ProgramId, MessageId)>) {
        if let Some(code) = self.opt_binaries.get(&code_hash).cloned() {
            for (candidate_id, init_message_id) in candidates {
                if !self.actors.contains_key(&candidate_id) {
                    let code = Code::try_new(code.clone(), 1, |_| ConstantCostRules::default())
                        .expect("Program can't be constructed with provided code");

                    let code_and_id: InstrumentedCodeAndId =
                        CodeAndId::from_parts_unchecked(code, code_hash).into();
                    let (code, code_id) = code_and_id.into_parts();
                    let candidate = CoreProgram::new(candidate_id, code);
                    self.store_new_actor(
                        candidate_id,
                        Program::new(candidate, code_id, Default::default()),
                        Some(init_message_id),
                    );
                } else {
                    logger::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            logger::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_hash
            );
            for (invalid_candidate_id, _) in candidates {
                self.actors
                    .insert(invalid_candidate_id, (Actor::Dormant, 0));
            }
        }
    }

    fn stop_processing(&mut self, _dispatch: StoredDispatch, _gas_burned: u64) {
        panic!("Processing stopped. Used for on-chain logic only.")
    }
}
