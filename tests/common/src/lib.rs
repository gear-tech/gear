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
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::storage::ProgramStorage;
use gear_core::{
    message::{Message, MessageId},
    program::ProgramId,
};
use gear_core_runner::{
    Config, ExecutionOutcome, Ext, ExtMessage, InMemoryRunner, InitializeProgramInfo,
};
use std::collections::{BTreeMap, HashSet};

pub type InMemoryWasmRunner = InMemoryRunner<WasmtimeEnvironment<Ext>>;

pub struct InitProgram {
    pub program_id: Option<ProgramId>,
    pub source_id: Option<ProgramId>,
    pub code: Vec<u8>,
    pub message: Option<MessageBuilder>,
}

impl InitProgram {
    pub fn id<P: Into<ProgramId>>(mut self, id: P) -> Self {
        self.program_id = Some(id.into());
        self
    }

    pub fn source_id<P: Into<ProgramId>>(mut self, id: P) -> Self {
        self.source_id = Some(id.into());
        self
    }

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

        InitializeProgramInfo {
            new_program_id: self.program_id.unwrap_or_else(|| context.next_program_id()),
            source_id: self.source_id.unwrap_or_else(ProgramId::system),
            code: self.code,
            message,
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
    pub fn id<T: Into<MessageId>>(mut self, id: T) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }

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
    pub fn source<P: Into<ProgramId>>(mut self, source: P) -> Self {
        self.source = Some(source.into());
        self
    }

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

#[derive(Debug, PartialEq, Eq)]
pub enum RunResult {
    Normal,
    Trap(String),
    Waiting,
}

impl From<ExecutionOutcome> for RunResult {
    fn from(outcome: ExecutionOutcome) -> Self {
        match outcome {
            ExecutionOutcome::Normal => RunResult::Normal,
            ExecutionOutcome::Trap(s) => RunResult::Trap(String::from(s.unwrap_or(""))),
            ExecutionOutcome::Waiting => RunResult::Waiting,
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
    runner: InMemoryWasmRunner,
    program_id: u64,
    used_program_ids: HashSet<ProgramId>,
    message_id: u64,
    used_message_ids: HashSet<MessageId>,
    message_queue: Vec<Message>,
    log: Vec<Message>,
    outcomes: BTreeMap<MessageId, ExecutionOutcome>,
    gas_spent: BTreeMap<MessageId, u64>,
}

impl RunnerContext {
    pub fn new(runner: InMemoryWasmRunner) -> Self {
        Self {
            runner,
            program_id: 1,
            message_id: 1,
            ..Default::default()
        }
    }

    pub fn log(&self) -> &[Message] {
        &self.log
    }

    pub fn with_config(config: &Config) -> Self {
        Self::new(InMemoryWasmRunner::new(
            config,
            Default::default(),
            Default::default(),
            WasmtimeEnvironment::default(),
        ))
    }

    pub fn init_program<P>(&mut self, init_data: P)
    where
        P: Into<InitProgram>,
    {
        let info = init_data.into().into_init_program_info(self);
        let program_id = info.new_program_id;

        let result = self
            .runner()
            .init_program(info)
            .expect("Failed to init program");

        let mut log = vec![];
        result.messages.into_iter().for_each(|m| {
            let m = m.into_message(program_id);
            if !self.runner().storage().program_storage.exists(m.dest()) {
                log.push(m);
            }
        });

        self.log.append(&mut log);
    }

    pub fn init_program_with_reply<P, D>(&mut self, init_data: P) -> D
    where
        P: Into<InitProgram>,
        D: Decode,
    {
        let info = init_data.into().into_init_program_info(self);
        let message_id = info.message.id;
        let program_id = info.new_program_id;

        let result = self
            .runner()
            .init_program(info)
            .expect("Failed to init program");

        let mut log = vec![];
        result.messages.into_iter().for_each(|m| {
            let m = m.into_message(program_id);
            if !self.runner().storage().program_storage.exists(m.dest()) {
                log.push(m);
            }
        });

        if let Some(m) = result.reply {
            log.push(m.into_message(message_id, program_id, 0.into()));
        }

        self.log.append(&mut log);

        reply_or_panic(self.get_response_to(message_id))
    }

    pub fn init_program_with_report<P, D>(&mut self, init_data: P) -> RunReport<D>
    where
        P: Into<InitProgram>,
        D: Decode,
    {
        let info = init_data.into().into_init_program_info(self);
        let program_id = info.new_program_id;
        let message_id = info.message.id;

        let result = self
            .runner()
            .init_program(info)
            .expect("Failed to init program");

        let mut log = vec![];
        result.messages.into_iter().for_each(|m| {
            let m = m.into_message(program_id);
            if !self.runner().storage().program_storage.exists(m.dest()) {
                log.push(m);
            }
        });

        self.log.append(&mut log);

        let response = self.get_response_to(message_id);

        RunReport {
            result: result.outcome.into(),
            response,
            gas_spent: result.gas_spent,
        }
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

        let outcome = self
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
            result: outcome.into(),
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

    fn runner(&mut self) -> &mut InMemoryWasmRunner {
        &mut self.runner //_state.convert_to_runner()
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
        let mut messages: Vec<_> = self.message_queue.drain(..).collect();
        messages.push(message);
        let mut outcomes = BTreeMap::new();
        let mut gas_spent = BTreeMap::new();

        let mut log = vec![];
        let runner = self.runner();
        while let Some(message) = messages.pop() {
            let mut run_result = runner.run_next(message);
            runner.process_wait_list(&mut run_result);
            for new_message in run_result.messages.drain(..) {
                messages.push(new_message);
            }
            for (id, outcome) in run_result.outcomes {
                outcomes.insert(id, outcome);
            }
            for (id, gas) in run_result.gas_spent {
                gas_spent.insert(id, gas);
            }

            log.append(&mut run_result.log);
        }

        self.log.append(&mut log);

        self.message_queue.append(&mut messages);

        for (id, outcome) in outcomes {
            self.outcomes.insert(id, outcome);
        }
        for (id, gas) in gas_spent {
            self.gas_spent.insert(id, gas);
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
            runner: Default::default(),
            program_id: 1,
            used_program_ids: HashSet::new(),
            message_id: 1,
            used_message_ids: HashSet::new(),
            message_queue: Vec::new(),
            log: Vec::new(),
            outcomes: BTreeMap::new(),
            gas_spent: BTreeMap::new(),
        }
    }
}
