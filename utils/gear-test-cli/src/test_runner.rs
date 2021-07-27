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

use gear_test_sample::sample::{PayloadVariant, Test};
use regex::Regex;
use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use rti::runner::ExtRunner;

use gear_core::{message::Message, program::ProgramId, storage::Storage};

use gear_common::storage_queue::StorageQueue;

use frame_system as system;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<gear_runtime::Runtime>()
        .unwrap()
        .into()
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

const SOME_FIXED_USER: u64 = 1000001;

pub fn init_fixture(
    ext: &mut sp_io::TestExternalities,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<ExtRunner> {
    ext.execute_with(|| {
        // Dispatch a signed extrinsic.

        let mut runner = rti::runner::new();
        let mut nonce = 0;
        for program in test.programs.iter() {
            let code = std::fs::read(program.path.clone())
                .map_err(|e| anyhow::anyhow!("Error openinng {}: {}", program.path.clone(), e))?;

            let mut init_message = Vec::new();
            if let Some(init_msg) = &program.init_message {
                let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
                init_message = match init_msg {
                    PayloadVariant::Utf8(s) => {
                        // Insert ProgramId
                        if let Some(caps) = re.captures(s) {
                            let id = caps["id"].parse::<u64>().unwrap();
                            let s =
                                s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                            (s.clone().into_bytes()).to_vec()
                        } else {
                            init_msg.clone().into_raw()
                        }
                    }
                    _ => init_msg.clone().into_raw(),
                }
            }

            runner.init_program(
                SOME_FIXED_USER.into(),
                nonce,
                program.id.into(),
                code,
                init_message,
                u64::max_value(),
                0,
            )?;
            nonce += 1;
        }
        let fixture = &test.fixtures[fixture_no];
        for message in fixture.messages.iter() {
            let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
            let payload = match &message.payload {
                Some(PayloadVariant::Utf8(s)) => {
                    // Insert ProgramId
                    if let Some(caps) = re.captures(&s) {
                        let id = caps["id"].parse::<u64>().unwrap();
                        let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                        (s.clone().into_bytes()).to_vec()
                    } else {
                        message
                            .payload
                            .as_ref()
                            .expect("Checked above")
                            .clone()
                            .into_raw()
                    }
                }
                _ => message.payload.clone().unwrap_or_default().into_raw(),
            };

            runner.queue_message(
                SOME_FIXED_USER.into(),
                nonce,
                message.destination.into(),
                payload,
                1000000000,
                0,
            );
            nonce += 1;
        }

        Ok(runner)
    })
}

pub struct FinalState {
    pub message_queue: Vec<Message>,
    pub allocation_storage: ExtAllocationStorage,
    pub program_storage: ExtProgramStorage,
}

pub fn run(
    ext: &mut sp_io::TestExternalities,
    mut runner: ExtRunner,
    steps: Option<u64>,
) -> anyhow::Result<(FinalState, Vec<u8>)> {
    ext.execute_with(|| {
        if let Some(steps) = steps {
            for _ in 0..steps {
                runner.run_next()?;
            }
        } else {
            while runner.run_next()?.handled > 0 {}
        }

        let mut messages = Vec::new();

        let mut message_queue = StorageQueue::get("g::msg::".as_bytes().to_vec());
        while let Some(message) = message_queue.dequeue() {
            messages.push(message);
        }

        let (
            Storage {
                message_queue: _,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ) = runner.complete();

        Ok((
            FinalState {
                message_queue: messages,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ))
    })
}
