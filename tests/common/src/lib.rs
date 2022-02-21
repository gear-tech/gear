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

use codec::{Decode, Encode, Error as CodecError};
use core_processor::{
    common::{DispatchOutcome, ExecutableActor, JournalHandler},
    configs::BlockInfo,
    Ext,
};
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::{
    memory::PageNumber,
    message::{Dispatch, DispatchKind, Message, MessageId},
    program::{Program, ProgramId},
};
use std::collections::{BTreeMap, HashSet};

pub const EXISTENCE_DEPOSIT: u128 = 500;

pub struct InitProgram {
    pub program_id: Option<ProgramId>,
    pub source_id: Option<ProgramId>,
    pub code: Vec<u8>,
    pub message: Option<MessageBuilder>,
}

/// Message payload with pre-generated identifier and economic data.
#[derive(Clone)]
pub struct ExtMessage {
    /// Id of the message.
    pub id: MessageId,
    /// Message payload.
    pub payload: Vec<u8>,
    /// Gas limit for the message dispatch.
    pub gas_limit: u64,
    /// Value associated with the message.
    pub value: u128,
}

struct InitializeProgramInfo {
    new_program_id: ProgramId,
    code: Vec<u8>,
    message: Message,
}

#[derive(Debug, PartialEq)]
pub enum RunResult {
    Normal,
    Trap(String),
}

impl InitProgram {
    #[must_use]
    pub fn id<P: Into<ProgramId>>(mut self, id: P) -> Self {
        self.program_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn source_id<P: Into<ProgramId>>(mut self, id: P) -> Self {
        self.source_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn message<M: Into<MessageBuilder>>(mut self, message: M) -> Self {
        self.message = Some(message.into());
        self
    }

    fn into_init_program_info(self, context: &mut RunnerContext) -> InitializeProgramInfo {
        self.program_id
            .map(|id| context.used_program_ids.insert(id));

        let message = self
            .message
            .map(|msg| msg.into_ext(context))
            .unwrap_or_else(|| MessageBuilder::from(()).into_ext(context));

        let new_program_id = self.program_id.unwrap_or_else(|| context.next_program_id());

        InitializeProgramInfo {
            new_program_id,
            code: self.code,
            message: Message {
                id: message.id,
                source: self.source_id.unwrap_or_else(ProgramId::system),
                dest: new_program_id,
                payload: message.payload.into(),
                gas_limit: message.gas_limit,
                value: message.value,
                reply: None,
            },
        }
    }
}

impl<C: Into<Vec<u8>>> From<C> for InitProgram {
    fn from(code: C) -> Self {
        Self {
            program_id: None,
            source_id: None,
            code: code.into(),
            message: None,
        }
    }
}

pub struct MessageBuilder {
    pub id: Option<MessageId>,
    pub payload: Vec<u8>,
    pub gas_limit: Option<u64>,
    pub value: Option<u128>,
}

impl MessageBuilder {
    #[must_use]
    pub fn id<T: Into<MessageId>>(mut self, id: T) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }

    #[must_use]
    pub fn value(mut self, value: u128) -> Self {
        self.value = Some(value);
        self
    }

    pub fn destination<P: Into<ProgramId>>(self, destination: P) -> MessageDispatchBuilder {
        MessageDispatchBuilder {
            source: None,
            destination: Some(destination.into()),
            message: self,
        }
    }

    fn into_ext(self, context: &mut RunnerContext) -> ExtMessage {
        self.id.map(|id| context.used_message_ids.insert(id));
        ExtMessage {
            id: self.id.unwrap_or_else(|| context.next_message_id()),
            payload: self.payload,
            gas_limit: self.gas_limit.unwrap_or(u64::MAX),
            value: self.value.unwrap_or(0),
        }
    }
}

impl<E: Encode> From<E> for MessageBuilder {
    fn from(payload: E) -> Self {
        Self {
            id: None,
            payload: payload.encode(),
            gas_limit: None,
            value: None,
        }
    }
}

pub struct MessageDispatchBuilder {
    source: Option<ProgramId>,
    destination: Option<ProgramId>,
    pub message: MessageBuilder,
}

impl MessageDispatchBuilder {
    #[must_use]
    pub fn source<P: Into<ProgramId>>(mut self, source: P) -> Self {
        self.source = Some(source.into());
        self
    }

    #[must_use]
    pub fn destination<P: Into<ProgramId>>(mut self, destination: P) -> Self {
        self.destination = Some(destination.into());
        self
    }

    fn into_message(self, runner: &mut RunnerContext) -> Message {
        let ext_message = self.message.into_ext(runner);
        Message {
            id: ext_message.id,
            source: self.source.unwrap_or_else(ProgramId::system),
            dest: self.destination.unwrap_or_else(|| 1.into()),
            payload: ext_message.payload.into(),
            gas_limit: ext_message.gas_limit,
            value: ext_message.value,
            reply: None,
        }
    }
}

impl From<MessageBuilder> for MessageDispatchBuilder {
    fn from(message: MessageBuilder) -> Self {
        Self {
            source: None,
            destination: None,
            message,
        }
    }
}

impl<E: Encode> From<E> for MessageDispatchBuilder {
    fn from(payload: E) -> Self {
        Self {
            source: None,
            destination: None,
            message: payload.into(),
        }
    }
}

pub struct RunReport<D> {
    pub result: RunResult,
    pub response: Option<Result<D, Error>>,
    pub gas_spent: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Decode(CodecError),
    Panic,
}

pub struct RunnerContext {
    // Existing key can have a None value, which declares that program is terminated (like being in limbo).
    actors: BTreeMap<ProgramId, Option<ExecutableActor>>,
    wait_list: BTreeMap<MessageId, Dispatch>,
    program_id: u64,
    used_program_ids: HashSet<ProgramId>,
    message_id: u64,
    used_message_ids: HashSet<MessageId>,
    dispatch_queue: Vec<Dispatch>,
    log: Vec<Message>,
    outcomes: BTreeMap<MessageId, RunResult>,
    gas_spent: BTreeMap<MessageId, u64>,
}

struct Journal<'a> {
    context: &'a mut RunnerContext,
}

impl<'a> JournalHandler for Journal<'a> {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        match outcome {
            DispatchOutcome::Success(_) | DispatchOutcome::NoExecution(_) => {}
            DispatchOutcome::MessageTrap {
                message_id, trap, ..
            } => {
                self.context.outcomes.insert(
                    message_id,
                    RunResult::Trap(trap.unwrap_or("No message").to_string()),
                );
            }
            DispatchOutcome::InitSuccess { .. } => {}
            DispatchOutcome::InitFailure { program_id, .. } => {
                if let Some(prog) = self.context.actors.get_mut(&program_id) {
                    *prog = None;
                }
            }
        };
    }

    fn gas_burned(&mut self, message_id: MessageId, _origin: ProgramId, amount: u64) {
        self.context.gas_spent.insert(message_id, amount);
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, _value_destination: ProgramId) {
        self.context.actors.remove(&id_exited);
    }

    fn message_consumed(&mut self, _message_id: MessageId) {}

    fn send_dispatch(&mut self, _origin: MessageId, dispatch: Dispatch) {
        match dispatch.message.reply {
            Some((message_id, 0)) => {
                self.context.outcomes.insert(message_id, RunResult::Normal);
            }
            Some((message_id, _)) => {
                self.context
                    .outcomes
                    .insert(message_id, RunResult::Trap(String::new()));
            }
            _ => {}
        }

        if self.context.actors.contains_key(&dispatch.message.dest) {
            self.context.dispatch_queue.push(dispatch);
        } else {
            self.context.log.push(dispatch.message);
        }
    }

    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.context.wait_list.insert(dispatch.message.id, dispatch);
    }

    fn wake_message(&mut self, _origin: MessageId, program_id: ProgramId, message_id: MessageId) {
        let dispatch = self
            .context
            .wait_list
            .remove(&message_id)
            .expect("wait list entry not found");

        // only wake messages from program that owns them
        if program_id == dispatch.message.dest {
            self.context.dispatch_queue.push(dispatch);
        } else {
            self.context.wait_list.insert(message_id, dispatch);
        }
    }

    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        let maybe_actor = self
            .context
            .actors
            .get_mut(&program_id)
            .expect("program not found");

        if let Some(actor) = maybe_actor {
            actor.program.set_message_nonce(nonce);
        }
    }

    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        let actor = self
            .context
            .actors
            .get_mut(&program_id)
            .expect("program not found");

        if let Some(actor) = actor {
            if let Some(data) = data {
                let _ = actor.program.set_page(page_number, &data);
            } else {
                actor.program.remove_page(page_number);
            }
        } else {
            unreachable!("Update page can'be called for terminated program");
        }
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        if let Some(to) = to {
            if let Some(Some(actor)) = self.context.actors.get_mut(&from) {
                if actor.balance < value {
                    panic!("Actor {:?} balance is less then sent value", from);
                }

                actor.balance -= value;
            };

            if let Some(Some(actor)) = self.context.actors.get_mut(&to) {
                actor.balance += value;
            };
        };
    }
}

impl RunnerContext {
    pub fn new() -> Self {
        Self {
            program_id: 1,
            message_id: 1,
            ..Default::default()
        }
    }

    pub fn log(&self) -> &[Message] {
        &self.log
    }

    pub fn init_program<P>(&mut self, init_data: P) -> MessageId
    where
        P: Into<InitProgram>,
    {
        // get init info
        let InitializeProgramInfo {
            new_program_id,
            message,
            code,
            ..
        } = init_data.into().into_init_program_info(self);

        // store program
        let program = Program::new(new_program_id, code).expect("Failed to create program");
        let actor = ExecutableActor {
            program,
            balance: 0,
        };
        self.actors.insert(new_program_id, Some(actor.clone()));

        // generate disspatch
        let dispatch = Dispatch {
            kind: DispatchKind::Init,
            message,
            payload_store: None,
        };
        let message_id = dispatch.message.id;

        let journal = core_processor::process::<WasmtimeEnvironment<Ext>>(
            Some(actor),
            dispatch,
            BlockInfo {
                height: 1,
                timestamp: 1,
            },
            EXISTENCE_DEPOSIT,
        );

        core_processor::handle_journal(journal, &mut Journal { context: self });

        message_id
    }

    pub fn init_program_with_reply<P, D>(&mut self, init_data: P) -> D
    where
        P: Into<InitProgram>,
        D: Decode,
    {
        let message_id = self.init_program(init_data);
        reply_or_panic(self.get_response_to(message_id))
    }

    pub fn try_request<Msg, D>(&mut self, message: Msg) -> Option<Result<D, Error>>
    where
        Msg: Into<MessageDispatchBuilder>,
        D: Decode,
    {
        let message = message.into().into_message(self);
        let message_id = message.id;

        self.run(message);
        self.get_response_to(message_id)
    }

    pub fn request_report<Msg, D>(&mut self, message: Msg) -> RunReport<D>
    where
        Msg: Into<MessageDispatchBuilder>,
        D: Decode,
    {
        let message = message.into().into_message(self);
        let message_id = message.id;

        self.run(message);

        let result = self
            .outcomes
            .remove(&message_id)
            .expect("Unable to get message outcome");

        let gas_spent = self
            .gas_spent
            .remove(&message_id)
            .expect("Unable to get spent gas for program");

        let response = self.get_response_to(message_id);

        RunReport {
            response,
            result,
            gas_spent,
        }
    }

    pub fn request<Msg, D>(&mut self, message: Msg) -> D
    where
        Msg: Into<MessageDispatchBuilder>,
        D: Decode,
    {
        reply_or_panic(self.try_request(message))
    }

    pub fn try_request_batch<M, I, D>(&mut self, requests: I) -> Vec<Option<Result<D, Error>>>
    where
        M: Into<MessageDispatchBuilder>,
        I: IntoIterator<Item = M>,
        D: Decode,
    {
        let mut message_ids: Vec<MessageId> = Vec::new();

        for request in requests {
            let request = request.into().into_message(self);
            let message_id = request.id;

            message_ids.push(message_id);

            self.run(request);
        }

        message_ids
            .into_iter()
            .map(|id| self.get_response_to(id))
            .collect()
    }

    pub fn request_batch<M, I, D>(&mut self, requests: I) -> Vec<D>
    where
        M: Into<MessageDispatchBuilder>,
        I: IntoIterator<Item = M>,
        D: Decode,
    {
        self.try_request_batch(requests)
            .into_iter()
            .map(reply_or_panic)
            .collect()
    }

    pub fn get_response_to<M, D>(&mut self, id: M) -> Option<Result<D, Error>>
    where
        M: Into<MessageId>,
        D: Decode,
    {
        let id = id.into();

        self.log
            .iter()
            .find(|message| message.reply.map(|(to, _)| to == id).unwrap_or(false))
            .map(|message| {
                let (_, exit_code) = message
                    .reply
                    .expect("messages that are not replies get filtered above");

                if exit_code != 0 {
                    Err(Error::Panic)
                } else {
                    D::decode(&mut message.payload.as_ref()).map_err(Error::Decode)
                }
            })
    }

    fn next_message_id(&mut self) -> MessageId {
        while !self.used_message_ids.insert(self.message_id.into()) {
            self.message_id += 1;
        }
        let message_id = self.message_id.into();
        self.message_id += 1;
        message_id
    }

    fn next_program_id(&mut self) -> ProgramId {
        while !self.used_program_ids.insert(self.program_id.into()) {
            self.program_id += 1;
        }
        let program_id = self.program_id.into();
        self.program_id += 1;
        program_id
    }

    fn run(&mut self, message: Message) {
        self.dispatch_queue.push(Dispatch {
            message,
            kind: DispatchKind::Handle,
            payload_store: None,
        });

        while !self.dispatch_queue.is_empty() {
            let journal = {
                let messages = std::mem::take(&mut self.dispatch_queue);
                let actors = self.actors.clone();

                core_processor::process_many::<WasmtimeEnvironment<Ext>>(
                    actors,
                    messages,
                    BlockInfo {
                        height: 1,
                        timestamp: 1,
                    },
                    EXISTENCE_DEPOSIT,
                )
            };

            core_processor::handle_journal(journal, &mut Journal { context: self });
        }
    }
}

fn reply_or_panic<D: Decode>(response: Option<Result<D, Error>>) -> D {
    match response.expect("No reply for message") {
        Ok(reply) => reply,
        Err(Error::Decode(e)) => panic!("Failed to decode reply: {}", e),
        Err(Error::Panic) => panic!("Request processing error"),
    }
}

impl Default for RunnerContext {
    fn default() -> Self {
        Self {
            actors: BTreeMap::new(),
            wait_list: BTreeMap::new(),
            program_id: 1,
            used_program_ids: HashSet::new(),
            message_id: 1,
            used_message_ids: HashSet::new(),
            dispatch_queue: Vec::new(),
            log: Vec::new(),
            outcomes: BTreeMap::new(),
            gas_spent: BTreeMap::new(),
        }
    }
}
