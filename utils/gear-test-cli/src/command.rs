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

use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;
use std::{fs, path::Path};
use termion::{color, style};

use gear_core::{
    message::Message,
    program::ProgramId,
    storage::{AllocationStorage, ProgramStorage},
};
use gear_test_sample::sample;

use crate::test_runner;
use crate::GearTestCmd;

fn read_test_from_file<P: AsRef<Path>>(path: P) -> Result<sample::Test, String> {
    let file = fs::File::open(path.as_ref())
        .map_err(|e| format!("Error opening {}: {}", path.as_ref().display(), e))?;

    let u = serde_json::from_reader(file)
        .map_err(|e| format!("Error decoding {}: {}", path.as_ref().display(), e))?;

    Ok(u)
}

fn check_messages(
    messages: &[Message],
    expected_messages: &[sample::Message],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if expected_messages.len() != messages.len() {
        errors.push("Expectation error (messages count doesn't match)".to_string());
    } else {
        expected_messages
            .iter()
            .zip(messages.iter())
            .for_each(|(exp, msg)| {
                if ProgramId::from(exp.destination) != msg.dest {
                    errors.push(format!(
                        "Expectation error (destination doesn't match, expected: {}, found: {:?})",
                        exp.destination, msg.dest
                    ));
                }
                if exp
                    .payload
                    .as_ref()
                    .map(|payload| !payload.equals(msg.payload.as_ref()))
                    .unwrap_or(false)
                {
                    errors.push(format!(
                        "Expectation error (payload doesn't match, expected: {:?}, actual: {:?})",
                        encode_hex(&exp.payload.clone().unwrap_or_default().into_raw()),
                        encode_hex(&msg.payload.clone().into_raw()),
                    ));
                }
            });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_allocations(
    ext: &mut sp_io::TestExternalities,
    allocations: &ExtAllocationStorage,
    expected_pages: &[sample::AllocationStorage],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    ext.execute_with(|| {

        for page in expected_pages {
            if let Some(program_id) = allocations.get(page.page_num.into()) {
                if program_id != page.program_id.into() {
                    errors.push(format!(
                        "Expectation error (ProgramId doesn't match, expected: {:?}, found: {:?})\n",
                        program_id, page.program_id
                    ));
                }
            } else {
                errors.push(format!(
                                    "Expectation error (PageNumber({}) doesn't exist)",
                                    page.page_num,
                                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    })
}

fn check_memory(
    ext: &mut sp_io::TestExternalities,
    persistent_memory: &[u8],
    program_storage: &ExtProgramStorage,
    expected_memory: &[sample::MemoryVariant],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for case in expected_memory {
        match case {
            sample::MemoryVariant::Static(case) => {
                ext.execute_with(|| {
                    if let Some(id) = case.program_id {
                        if let Some(program) = program_storage.get(ProgramId::from(id)) {
                            if program.id() == ProgramId::from(id) {
                                let page = case.address / 65536;
                                if let Some(page_buf) = program.get_page((page as u32).into()) {
                                    if page_buf[case.address..case.address + case.bytes.len()]
                                        != case.bytes
                                    {
                                        errors.push(
                                            "Expectation error (Static memory doesn't match)"
                                                .to_string(),
                                        );
                                    }
                                } else {
                                    errors.push(
                                        "Expectation error (Incorrect static memory address)"
                                            .to_string(),
                                    );
                                }
                            }
                        }
                    }
                });
            }
            sample::MemoryVariant::Shared(case) => {
                let offset = 256 * 65536;
                if persistent_memory
                    [case.address - offset..case.address - offset + case.bytes.len()]
                    != case.bytes
                {
                    errors.push("Expectation error (Shared memory doesn't match)".to_string());
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

impl GearTestCmd {
    /// Runs tests from json files.
    pub fn run(&self, _config: Configuration) -> sc_cli::Result<()> {
        let mut total_failed = 0i32;
        let mut tests = Vec::new();
        for path in &self.input {
            if path.is_dir() {
                for entry in path.read_dir().expect("read_dir call failed") {
                    if let Ok(entry) = entry {
                        tests.push(read_test_from_file(&entry.path())?);
                    }
                }
            } else {
                tests.push(read_test_from_file(&path)?);
            }
        }

        let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
        println!("Total fixtures: {}", total_fixtures);

        for test in tests {
            for fixture_no in 0..test.fixtures.len() {
                for exp in &test.fixtures[fixture_no].expected {
                    let mut ext = crate::test_runner::new_test_ext();
                    let output = match test_runner::init_fixture(&mut ext, &test, fixture_no) {
                        Ok(initialized_fixture) => {
                            match test_runner::run(&mut ext, initialized_fixture, exp.step) {
                                Ok((final_state, persistent_memory)) => {
                                    let mut errors = Vec::new();
                                    if let Some(messages) = &exp.messages {
                                        if let Err(msg_errors) =
                                            check_messages(&final_state.message_queue, messages)
                                        {
                                            errors.extend(msg_errors);
                                        }
                                    }
                                    if let Some(alloc) = &exp.allocations {
                                        if let Err(alloc_errors) = check_allocations(
                                            &mut ext,
                                            &final_state.allocation_storage,
                                            alloc,
                                        ) {
                                            errors.extend(alloc_errors);
                                        }
                                    }
                                    if let Some(mem) = &exp.memory {
                                        if let Err(mem_errors) = check_memory(
                                            &mut ext,
                                            &persistent_memory,
                                            &final_state.program_storage,
                                            mem,
                                        ) {
                                            errors.extend(mem_errors);
                                        }
                                    }

                                    if !errors.is_empty() {
                                        total_failed += 1;
                                        errors.insert(0, format!("{}", color::Fg(color::Red)));
                                        errors.insert(errors.len(), format!("{}", style::Reset));
                                        errors.join("\n")
                                    } else {
                                        format!("{}Ok{}", color::Fg(color::Green), style::Reset)
                                    }
                                }
                                Err(e) => {
                                    total_failed += 1;
                                    format!(
                                        "{}Running error ({}){}",
                                        color::Fg(color::Red),
                                        e,
                                        style::Reset
                                    )
                                }
                            }
                        }
                        Err(e) => {
                            total_failed += 1;
                            format!(
                                "{}Initialization error ({}){}",
                                color::Fg(color::Red),
                                e,
                                style::Reset
                            )
                        }
                    };

                    println!(
                        "Fixture {}{}{}: {}",
                        style::Bold,
                        test.fixtures[fixture_no].title,
                        style::Reset,
                        output
                    );
                }
            }
        }

        if total_failed > 0 {
            Err(sc_cli::Error::Application(
                format!(
                    "{}/{} fixtures failed... See log above.",
                    total_failed, total_fixtures
                )
                .into(),
            ))
        } else {
            Ok(())
        }
    }
}

impl CliConfiguration for GearTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}
