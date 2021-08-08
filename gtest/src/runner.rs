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

use crate::sample::{PayloadVariant, Test};
use gear_core::{
    message::Message,
    program::{Program, ProgramId},
    storage::{
        InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList, MessageMap, MessageQueue,
        ProgramStorage, Storage, WaitList,
    },
};
use gear_core_runner::{Config, ExtMessage, InitializeProgramInfo, MessageDispatch, Runner};
use gear_node_rti::ext::{ExtMessageQueue, ExtProgramStorage, ExtWaitList};
use std::fmt::Write;

use regex::Regex;

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

const SOME_FIXED_USER: u64 = 1000001;

pub trait CollectState {
    fn collect(self) -> FinalState;
}

impl CollectState for Storage<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList> {
    fn collect(self) -> FinalState {
        FinalState {
            log: self.message_queue.log().to_vec(),
            messages: self.message_queue.into(),
            program_storage: self.program_storage.into(),
            wait_list: self.wait_list.into(),
        }
    }
}

impl CollectState for Storage<ExtMessageQueue, ExtProgramStorage, ExtWaitList> {
    fn collect(self) -> FinalState {
        let log = self.message_queue.log;

        let mut messages = Vec::new();

        let mut message_queue =
            common::storage_queue::StorageQueue::get("g::msg::".as_bytes().to_vec());
        while let Some(message) = message_queue.dequeue() {
            messages.push(message);
        }

        FinalState {
            log,
            messages,
            // TODO: iterate program storage to list programs here
            program_storage: Vec::new(),
            wait_list: self.wait_list.into(),
        }
    }
}

pub fn init_fixture<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList>(
    storage: Storage<MQ, PS, WL>,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<Runner<MQ, PS, WL>> {
    let mut runner = Runner::new(&Config::default(), storage);
    let mut nonce = 0;
    for program in test.programs.iter() {
        let code = std::fs::read(program.path.clone())?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => {
                    // Insert ProgramId
                    if let Some(caps) = re.captures(s) {
                        let id = caps["id"].parse::<u64>().unwrap();
                        let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                        (s.clone().into_bytes()).to_vec()
                    } else {
                        init_msg.clone().into_raw()
                    }
                }
                _ => init_msg.clone().into_raw(),
            }
        }
        runner.init_program(InitializeProgramInfo {
            new_program_id: program.id.into(),
            source_id: SOME_FIXED_USER.into(),
            code,
            message: ExtMessage {
                id: nonce.into(),
                payload: init_message,
                gas_limit: program.init_gas_limit.unwrap_or(u64::MAX),
                value: program.init_value.unwrap_or(0) as _,
            },
        })?;

        nonce += 1;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
        let payload = match &message.payload {
            Some(PayloadVariant::Utf8(s)) => {
                // Insert ProgramId
                if let Some(caps) = re.captures(s) {
                    let id = caps["id"].parse::<u64>().unwrap();
                    let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                    (s.clone().into_bytes()).to_vec()
                } else {
                    message
                        .payload
                        .as_ref()
                        .expect("Checked above.")
                        .clone()
                        .into_raw()
                }
            }
            _ => message
                .payload
                .as_ref()
                .map(|payload| payload.clone().into_raw())
                .unwrap_or_default(),
        };
        runner.queue_message(MessageDispatch {
            source_id: 0.into(),
            destination_id: message.destination.into(),
            data: ExtMessage {
                id: nonce.into(),
                payload,
                gas_limit: message.gas_limit.unwrap_or(u64::MAX),
                value: message.value.unwrap_or_default() as _,
            },
        });

        nonce += 1;
    }

    Ok(runner)
}

pub struct FinalState {
    pub messages: Vec<Message>,
    pub log: Vec<Message>,
    pub program_storage: Vec<Program>,
    pub wait_list: MessageMap,
}

pub fn run<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList>(
    mut runner: Runner<MQ, PS, WL>,
    steps: Option<u64>,
) -> (FinalState, anyhow::Result<()>)
where
    Storage<MQ, PS, WL>: CollectState,
{
    let mut result = Ok(());
    if let Some(steps) = steps {
        for step_no in 0..steps {
            let run_result = runner.run_next(u64::MAX);

            log::info!("step: {}", step_no + 1);

            if run_result.any_traps() && step_no + 1 == steps {
                result = Err(anyhow::anyhow!("Runner resulted in a trap"));
            }
        }
    } else {
        loop {
            let run_result = runner.run_next(u64::MAX);

            if run_result.handled == 0 {
                break;
            }

            log::info!("handled: {}", run_result.handled);
        }
    }

    let storage = runner.complete();

    (storage.collect(), result)
}
