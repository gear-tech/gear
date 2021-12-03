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

use crate::js::{MetaData, MetaType};
use crate::sample::{PayloadVariant, Test};
use gear_backend_common::Environment;
use gear_core::storage::ProgramStorage;
use gear_core::{
    message::Message,
    program::{Program, ProgramId},
    storage::{InMemoryStorage, Storage, StorageCarrier},
};
use gear_core_runner::{Config, ExecutionOutcome, ExtMessage, InitializeProgramInfo, Runner};
use gear_node_runner::{Ext, ExtStorage};
use sp_core::{crypto::Ss58Codec, hexdisplay::AsBytesRef, sr25519::Public};
use sp_keyring::sr25519::Keyring;
use std::collections::VecDeque;
use std::fmt::Write;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

type WasmRunner<SC> = Runner<SC, gear_backend_wasmtime::WasmtimeEnvironment<Ext>>;

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

fn parse_payload(payload: String) -> String {
    let program_id_regex = Regex::new(r"\{(?P<id>[0-9]+)\}").unwrap();
    let account_regex = Regex::new(r"\{(?P<id>[a-z]+)\}").unwrap();
    let ss58_regex = Regex::new(r"\{(?P<id>[A-Za-z0-9]+)\}").unwrap();

    // Insert ProgramId
    let mut s = payload;
    while let Some(caps) = program_id_regex.captures(&s) {
        let id = caps["id"].parse::<u64>().unwrap();
        s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
    }

    while let Some(caps) = account_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &encode_hex(
                ProgramId::from_slice(Keyring::from_str(id).unwrap().to_h256_public().as_bytes())
                    .as_slice(),
            ),
        );
    }

    while let Some(caps) = ss58_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &encode_hex(
                ProgramId::from_slice(Public::from_ss58check(id).unwrap().as_bytes_ref())
                    .as_slice(),
            ),
        );
    }

    s
}

const SOME_FIXED_USER: u64 = 1000001;

pub trait CollectState {
    fn collect(self) -> FinalState;
}

impl CollectState for InMemoryStorage {
    fn collect(self) -> FinalState {
        FinalState {
            log: vec![],
            messages: Vec::new(),
            program_storage: self.program_storage.into(),
        }
    }
}

impl CollectState for ExtStorage {
    fn collect(self) -> FinalState {
        let program_storage = self.program_storage;

        let mut messages = Vec::new();
        let mut message_queue =
            common::storage_queue::StorageQueue::get(common::STORAGE_MESSAGE_PREFIX);
        while let Some(message) = message_queue.dequeue() {
            messages.push(message);
        }

        FinalState {
            log: vec![],
            messages,
            program_storage: program_storage.iter().collect(),
        }
    }
}

/// Initializes programs defined in `test.programs` and queues all the messages from `test.fixtures[fixture_no]`.
///
/// Program initialization and queueing messages is performed by [`Runner`],
/// which uses `storage` as a storage manager. This storage is actually returned to the function caller to be later used to run queued messages.
pub fn init_fixture<SC: StorageCarrier>(
    storage: Storage<SC::PS>,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<(WasmRunner<SC>, Vec<Message>, Vec<Message>)> {
    let mut runner = Runner::new(
        &Config::default(),
        storage,
        Default::default(),
        gear_backend_wasmtime::WasmtimeEnvironment::<Ext>::default(),
    );
    let mut messages = Vec::new();
    let mut log = vec![];
    let mut nonce = 0;
    for program in test.programs.iter() {
        let program_path = program.path.clone();
        let code = std::fs::read(&program_path)?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => parse_payload(s.clone()).into_bytes(),
                PayloadVariant::Custom(v) => {
                    let meta_type = MetaType::InitInput;

                    let payload =
                        parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                    let json = MetaData::Json(payload);

                    let wasm = program_path.replace(".wasm", ".meta.wasm");

                    json.convert(&wasm, &meta_type)
                        .expect("Unable to get bytes")
                        .into_bytes()
                }
                _ => init_msg.clone().into_raw(),
            }
        }
        let mut init_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &program.source {
            init_source = source.to_program_id();
        }

        let message_id = nonce.into();
        let program_id = program.id.to_program_id();
        let result = runner.init_program(InitializeProgramInfo {
            new_program_id: program_id,
            source_id: init_source,
            code,
            message: ExtMessage {
                id: message_id,
                payload: init_message,
                gas_limit: program.init_gas_limit.unwrap_or(u64::MAX),
                value: program.init_value.unwrap_or(0) as _,
            },
        })?;

        if let ExecutionOutcome::Trap(explanation) = result.outcome {
            return Err(anyhow::anyhow!("Trap during `init`: {:?}", explanation));
        }

        let storage: Storage<SC::PS> = runner.storage();
        result.messages.into_iter().for_each(|m| {
            let m = m.into_message(program_id);
            if !storage.program_storage.exists(m.dest()) {
                log.push(m);
            } else {
                messages.push(m);
            }
        });

        if let Some(m) = result.reply {
            if !storage.program_storage.exists(init_source) {
                log.push(m.into_message(message_id, program_id, init_source));
            } else {
                messages.push(m.into_message(message_id, program_id, init_source));
            }
        }

        nonce += 1;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        let payload = match &message.payload {
            Some(PayloadVariant::Utf8(s)) => {
                // Insert ProgramId
                parse_payload(s.clone()).as_bytes().to_vec()
            }
            Some(PayloadVariant::Custom(v)) => {
                let meta_type = MetaType::HandleInput;

                let payload =
                    parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                let json = MetaData::Json(payload);

                let wasm = test
                    .programs
                    .iter()
                    .filter(|p| p.id == message.destination)
                    .last()
                    .expect("Program not found")
                    .path
                    .clone()
                    .replace(".wasm", ".meta.wasm");

                json.convert(&wasm, &meta_type)
                    .expect("Unable to get bytes")
                    .into_bytes()
            }
            _ => message
                .payload
                .as_ref()
                .map(|payload| payload.clone().into_raw())
                .unwrap_or_default(),
        };
        let mut message_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &message.source {
            message_source = source.to_program_id();
        }
        messages.push(Message {
            id: nonce.into(),
            source: message_source,
            dest: message.destination.to_program_id(),
            payload: payload.into(),
            gas_limit: message.gas_limit.unwrap_or(u64::MAX),
            value: message.value.unwrap_or_default() as _,
            reply: None,
        });

        nonce += 1;
    }

    Ok((runner, messages, log))
}

#[derive(Clone, Debug)]
pub struct FinalState {
    pub messages: Vec<Message>,
    pub log: Vec<Message>,
    pub program_storage: Vec<Program>,
}

/// Runs queued messages using `runner`.
///
/// Param `steps` is needed to control an amount of message processes. This is actually needed
/// to check the interim state during tests. For example, a user could want to check the state
/// after processing 2 out of 10 messages in the message queue.
///
/// If `steps` is `None`, then all the messages in the queue will be processed.
pub fn run<SC: StorageCarrier, E: Environment<Ext>>(
    mut runner: Runner<SC, E>,
    mut messages: VecDeque<Message>,
    mut log: Vec<Message>,
    steps: Option<usize>,
) -> Vec<(FinalState, anyhow::Result<()>)>
where
    Storage<SC::PS>: CollectState,
{
    let mut results = Vec::new();

    let storage = runner.storage();

    let mut final_state = storage.collect();

    final_state.messages = messages.clone().into();

    final_state.log = log.clone();
    results.push((final_state, Ok(())));
    let mut _result = Ok(());
    if let Some(steps) = steps {
        for step_no in 0..steps {
            runner.set_block_height(step_no as _);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            runner.set_block_timestamp(timestamp as _);

            if let Some(m) = messages.pop_front() {
                let mut run_result = runner.run_next(m);
                runner.process_wait_list(&mut run_result);

                log::info!("step: {}", step_no + 1);

                if run_result.any_traps() && step_no + 1 == steps {
                    _result = Err(anyhow::anyhow!("Runner resulted in a trap"));
                }

                messages.append(&mut run_result.messages.into());
                log.append(&mut run_result.log);
            }

            let storage = runner.storage();

            let mut final_state = storage.collect();

            final_state.messages = messages.clone().into();

            final_state.log = log.clone();

            results.push((final_state, Ok(())));
        }
    } else {
        while let Some(m) = messages.pop_front() {
            let mut run_result = runner.run_next(m);
            runner.process_wait_list(&mut run_result);

            messages.append(&mut run_result.messages.into());
            log.append(&mut run_result.log);

            let storage = runner.storage();

            let mut final_state = storage.collect();
            final_state.messages = messages.clone().into();

            final_state.log = log.clone();

            results.push((final_state, Ok(())));
        }
    }

    results
}
