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
use gear_core::storage::InMemoryStorage;
use gear_core::{message::MessageId, program::ProgramId};
use gear_core_runner::{
    Config, ExecutionOutcome, Ext, ExtMessage, InMemoryRunner, InitializeProgramInfo,
    MessageDispatch,
};
use std::collections::HashSet;

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

    fn into_message_dispatch(self, runner: &mut RunnerContext) -> MessageDispatch {
        MessageDispatch {
            source_id: self.source.unwrap_or_else(ProgramId::system),
            destination_id: self.destination.unwrap_or_else(|| 1.into()),
            data: self.message.into_ext(runner),
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
}

impl From<ExecutionOutcome> for RunResult {
    fn from(outcome: ExecutionOutcome) -> Self {
        match outcome {
            ExecutionOutcome::Normal => RunResult::Normal,
            ExecutionOutcome::Trap(s) => RunResult::Trap(String::from(s.unwrap_or(""))),
        }
    }
}

pub struct RunReport<D> {
    pub result: RunResult,
    pub response: Option<Result<D, Error>>,
    pub gas_left: u64,
    pub gas_spent: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Decode(CodecError),
    Panic,
}

pub struct RunnerContext {
    runner_state: RunnerState,
    program_id: u64,
    used_program_ids: HashSet<ProgramId>,
    message_id: u64,
    used_message_ids: HashSet<MessageId>,
}

impl RunnerContext {
    pub fn new(runner: InMemoryWasmRunner) -> Self {
        Self {
            runner_state: RunnerState::Runner(runner),
            program_id: 1,
            used_program_ids: HashSet::new(),
            message_id: 1,
            used_message_ids: HashSet::new(),
        }
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

        self.runner()
            .init_program(info)
            .expect("Failed to init program");
    }

    pub fn init_program_with_reply<P, D>(&mut self, init_data: P) -> D
    where
        P: Into<InitProgram>,
        D: Decode,
    {
        let info = init_data.into().into_init_program_info(self);
        let message_id = info.message.id;

        self.runner()
            .init_program(info)
            .expect("Failed to init program");

        reply_or_panic(self.get_response_to(message_id))
    }

    pub fn init_program_with_report<P, D>(&mut self, init_data: P) -> RunReport<D>
    where
        P: Into<InitProgram>,
        D: Decode,
    {
        let info = init_data.into().into_init_program_info(self);
        let message_id = info.message.id;

        let result = self
            .runner()
            .init_program(info)
            .expect("Failed to init program");

        let response = self.get_response_to(message_id);

        RunReport {
            result: result.outcome.into(),
            response,
            gas_left: result.gas_left,
            gas_spent: result.gas_spent,
        }
    }

    pub fn try_request<Msg, D>(&mut self, message: Msg) -> Option<Result<D, Error>>
    where
        Msg: Into<MessageDispatchBuilder>,
        D: Decode,
    {
        let message_dispatch = message.into().into_message_dispatch(self);
        let message_id = message_dispatch.data.id;

        let runner = self.runner();

        runner.queue_message(message_dispatch);

        while runner.run_next(u64::MAX).handled > 0 {}

        self.get_response_to(message_id)
    }

    pub fn request_report<Msg, D>(&mut self, message: Msg) -> RunReport<D>
    where
        Msg: Into<MessageDispatchBuilder>,
        D: Decode,
    {
        let message_dispatch = message.into().into_message_dispatch(self);
        let message_id = message_dispatch.data.id;
        let program_id = message_dispatch.source_id;

        let runner = self.runner();

        runner.queue_message(message_dispatch);

        let mut result = loop {
            let result = runner.run_next(u64::MAX);
            if result.handled > 0 {
                break result;
            }
        };

        let outcome = result
            .outcomes
            .remove(&message_id)
            .expect("Unable to get message outcome");

        let gas_left = result
            .gas_left
            .into_iter()
            .find_map(|(id, left)| if id == program_id { Some(left) } else { None })
            .expect("Unable to get remaining gas for program");

        let gas_spent = result
            .gas_spent
            .into_iter()
            .find_map(|(id, spent)| if id == program_id { Some(spent) } else { None })
            .expect("Unable to get spent gas for program");

        let response = self.get_response_to(message_id);

        RunReport {
            response,
            result: outcome.into(),
            gas_left,
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
            let request = request.into().into_message_dispatch(self);
            let message_id = request.data.id;

            message_ids.push(message_id);
            self.runner().queue_message(request);
        }

        while self.runner().run_next(u64::MAX).handled != 0 {}

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
        self.storage()
            .log
            .get()
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

    pub fn storage(&mut self) -> &InMemoryStorage {
        self.runner_state.convert_to_storage()
    }

    fn runner(&mut self) -> &mut InMemoryWasmRunner {
        self.runner_state.convert_to_runner()
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
            runner_state: RunnerState::Uninitialzied,
            program_id: 1,
            used_program_ids: HashSet::new(),
            message_id: 1,
            used_message_ids: HashSet::new(),
        }
    }
}

enum RunnerState {
    Runner(InMemoryWasmRunner),
    Storage(InMemoryStorage, Config),
    Uninitialzied,
}

impl RunnerState {
    fn convert_to_runner(&mut self) -> &mut InMemoryWasmRunner {
        if let Self::Runner(runner) = self {
            runner
        } else {
            *self = match std::mem::take(self) {
                Self::Storage(storage, config) => Self::Runner(InMemoryWasmRunner::new(
                    &config,
                    storage,
                    Default::default(),
                    WasmtimeEnvironment::default(),
                )),
                _ => Self::Runner(InMemoryWasmRunner::default()),
            };

            self.convert_to_runner()
        }
    }

    fn convert_to_storage(&mut self) -> &InMemoryStorage {
        if let Self::Storage(storage, _) = self {
            storage
        } else {
            *self = if let Self::Runner(runner) = std::mem::take(self) {
                let config = Config {
                    max_pages: runner.max_pages(),
                    alloc_cost: runner.alloc_cost(),
                    mem_grow_cost: runner.mem_grow_cost(),
                    init_cost: runner.init_cost(),
                    load_page_cost: runner.load_page_cost(),
                };
                let storage = runner.complete();
                Self::Storage(storage, config)
            } else {
                Self::Storage(InMemoryStorage::default(), Config::default())
            };

            self.convert_to_storage()
        }
    }
}

impl Default for RunnerState {
    fn default() -> Self {
        Self::Uninitialzied
    }
}
