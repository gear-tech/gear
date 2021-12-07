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
use core_runner::{
    AllocationsConfig, BlockInfo, CoreRunner, EntryPoint, ExecutionOutcome, ExecutionSettings, Ext,
    RunResult,
};
use gear_core::storage::ProgramStorage;
use gear_core::{
    message::{IncomingMessage, Message, MessageId},
    program::{Program, ProgramId},
    storage::{InMemoryStorage, Storage, StorageCarrier},
};
use sp_core::{crypto::Ss58Codec, hexdisplay::AsBytesRef, sr25519::Public};
use sp_keyring::sr25519::Keyring;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt::Write;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

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

impl CollectState for gear_node_runner::ExtStorage {
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

#[derive(Clone, Debug)]
pub struct InitMessage {
    pub program_id: ProgramId,
    pub program_code: Vec<u8>,
    pub message: IncomingMessage,
}

pub fn init_program(message: InitMessage) -> anyhow::Result<RunResult> {
    let program = Program::new(message.program_id, message.program_code, Default::default())?;

    if program.static_pages() > AllocationsConfig::new().max_pages.raw() {
        return Err(anyhow::anyhow!(
            "Error initialisation: memory limit exceeded"
        ));
    }

    let mut env = gear_backend_wasmtime::WasmtimeEnvironment::<Ext>::new();

    let code = program.code();

    let prog = program.clone();

    println!("EXECUTE THIS INIT: {:?}", message.message.clone());

    let result = CoreRunner::run(
        &mut env,
        prog,
        message.message,
        &gear_core::gas::instrument(code)
            .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))?,
        ExecutionSettings::new(
            EntryPoint::Init,
            BlockInfo {
                height: 0,
                timestamp: 0,
            },
        ),
    );

    println!("{:?}\n", result);

    Ok(result)
}

pub fn init_fixture<SC: StorageCarrier>(
    mut storage: Storage<SC::PS>,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<(Storage<SC::PS>, Vec<Message>, Vec<Message>)>
where
    Storage<SC::PS>: CollectState,
{
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

        let result = init_program(InitMessage {
            program_id: program_id,
            program_code: code,
            message: IncomingMessage::new(
                message_id,
                init_source,
                init_message.into(),
                program.init_gas_limit.unwrap_or(u64::MAX),
                program.init_value.unwrap_or(0) as u128,
            ),
        })?;

        let _ = storage.program_storage.set(result.program);

        if result.outcome.was_trap() {
            if let ExecutionOutcome::Trap(explanation) = result.outcome {
                return Err(anyhow::anyhow!("Trap during `init`: {:?}", explanation));
            }
        }

        result.messages.into_iter().for_each(|m| {
            if !storage.program_storage.exists(m.dest()) {
                log.push(m);
            } else {
                messages.push(m);
            }
        });

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

    Ok((storage, messages, log))
}

#[derive(Clone, Debug)]
pub struct FinalState {
    pub messages: Vec<Message>,
    pub log: Vec<Message>,
    pub program_storage: Vec<Program>,
}

pub fn process_wait_list(
    wait_list: &mut BTreeMap<(ProgramId, MessageId), Message>,
    msg: IncomingMessage,
    result: &mut RunResult,
) {
    if result.outcome.wait_interrupt() {
        wait_list.insert(
            (result.program.id(), msg.id()),
            msg.into_message(result.program.id()),
        );
    }

    // Messages to be added back to the queue
    let msgs: Vec<_> = result
        .awakening
        .iter()
        .filter_map(|msg_id| wait_list.remove(&(result.program.id(), *msg_id)))
        .collect();

    for msg in msgs {
        result.messages.push(msg);
    }
}

pub fn run<SC: StorageCarrier>(
    storage: &mut Storage<SC::PS>,
    messages: VecDeque<Message>,
    log: Vec<Message>,
    wait_list: &mut BTreeMap<(ProgramId, MessageId), Message>,
    steps: Option<usize>,
) -> Vec<(FinalState, anyhow::Result<()>)>
where
    Storage<SC::PS>: CollectState,
{
    let mut env = gear_backend_wasmtime::WasmtimeEnvironment::<Ext>::new();
    let mut results = Vec::new();

    let mut messages = messages;
    let mut log = log;

    let mut final_state = storage.clone().collect();

    final_state.messages = messages.clone().into();
    final_state.log = log.clone();

    results.push((final_state.clone(), Ok(())));

    let mut _result = Ok(());

    if let Some(steps) = steps {
        for step_no in 0..steps {
            let block_height = step_no as u32;
            let block_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            if let Some(m) = messages.pop_front() {
                let program = storage
                    .program_storage
                    .get(m.dest())
                    .expect("Can't find program");
                let code = gear_core::gas::instrument(program.code())
                    .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))
                    .expect("Can't instrument code");
                let entry = if let Some(_) = m.reply() {
                    EntryPoint::HandleReply
                } else {
                    EntryPoint::Handle
                };

                println!("EXECUTE THIS MESSAGE: {:?}", m);

                let message: IncomingMessage = m.into();

                let settings = ExecutionSettings::new(
                    entry,
                    BlockInfo {
                        height: block_height,
                        timestamp: block_timestamp,
                    },
                );

                let mut result =
                    CoreRunner::run(&mut env, program, message.clone(), &code, settings);

                println!("{:?}\n", result);

                storage
                    .program_storage
                    .set(result.program.clone())
                    .expect("Can't find program");

                process_wait_list(wait_list, message, &mut result);

                log::debug!("step: {}", step_no + 1);

                if result.outcome.was_trap() && step_no + 1 == steps {
                    _result = Err(anyhow::anyhow!("Runner resulted in a trap"));
                }

                for m in result.messages {
                    if !storage.program_storage.exists(m.dest()) {
                        log.push(m);
                    } else {
                        messages.push_back(m);
                    }
                }
            }

            final_state.messages = messages.clone().into();

            final_state.log = log.clone();

            results.push((final_state.clone(), Ok(())));
        }
    } else {
        let mut counter = 0;
        while let Some(m) = messages.pop_front() {
            let program = storage
                .program_storage
                .get(m.dest())
                .expect("Can't find program");
            let code = gear_core::gas::instrument(program.code())
                .map_err(|e| anyhow::anyhow!("Error instrumenting: {:?}", e))
                .expect("Can't instrument code");

            let entry = if let Some(_) = m.reply() {
                EntryPoint::HandleReply
            } else {
                EntryPoint::Handle
            };

            let message: IncomingMessage = m.into();

            let block_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            let settings = ExecutionSettings::new(
                entry,
                BlockInfo {
                    height: counter,
                    timestamp: block_timestamp,
                },
            );

            counter += 1;

            let mut result = CoreRunner::run(&mut env, program, message.clone(), &code, settings);

            storage
                .program_storage
                .set(result.program.clone())
                .expect("Can't find program");

            process_wait_list(wait_list, message, &mut result);

            for m in result.messages {
                if !storage.program_storage.exists(m.dest()) {
                    log.push(m);
                } else {
                    messages.push_back(m);
                }
            }

            // let mut final_state = storage.clone().collect();

            final_state.messages = messages.clone().into();

            final_state.log = log.clone();

            results.push((final_state.clone(), Ok(())));
        }
    }

    results
}
