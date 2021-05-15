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

use regex::Regex;
use codec::Decode;
use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use rti::runner::ExtRunner;
use test_gear_sample::sample::{Test, PayloadVariant};

use gear_core::{message::Message, storage::Storage, program::ProgramId};

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

pub fn init_fixture(
    ext: &mut sp_io::TestExternalities,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<ExtRunner> {
    ext.execute_with(|| {
        // Dispatch a signed extrinsic.

        let mut runner = rti::runner::new();
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
                            let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                            (s.clone().into_bytes()).to_vec()
                        } else {
                            init_msg.clone().into_raw()
                        }
                    }
                    _ => init_msg.clone().into_raw(),
                }
            }

            runner.init_program(program.id.into(), code, init_message, u64::max_value(), 0)?;
        }
        let fixture = &test.fixtures[fixture_no];
        for message in fixture.messages.iter() {
            let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
            let payload = match &message.payload {
                PayloadVariant::Utf8(s) => {
                    // Insert ProgramId
                    if let Some(caps) = re.captures(&s) {
                        let id = caps["id"].parse::<u64>().unwrap();
                        let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                        (s.clone().into_bytes()).to_vec()
                    } else {
                        message.payload.clone().into_raw()
                    }
                }
                _ => message.payload.clone().into_raw(),
            };

            runner.queue_message(
                message.destination.into(),
                payload,
                Some(u64::max_value()),
                0,
            )
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
            while runner.run_next()? > 0 {}
        }
        let message_queue = sp_io::storage::get(b"g::msg")
            .map(|val| Vec::<Message>::decode(&mut &val[..]).expect("values encoded correctly"))
            .unwrap_or_default();

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
                message_queue,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ))
    })
}
