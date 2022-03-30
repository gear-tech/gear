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

#![allow(unused_must_use)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::util::{get_dispatch_queue, new_test_ext, process_queue};
use crate::GearRuntimeTestCmd;
use colored::{ColoredString, Colorize};

use gear_runtime::{Origin, Runtime};

use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    message::{DispatchKind, GasLimit, StoredDispatch, StoredMessage},
    program::Program as CoreProgram,
};

use gear_common::{DAGBasedLedger, Origin as _};
use gear_test::{
    check::read_test_from_file,
    js::{MetaData, MetaType},
    proc::*,
    sample,
    sample::PayloadVariant,
};
use pallet_gear::Pallet as GearPallet;
use pallet_gear_debug::DebugData;
use rayon::prelude::*;
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;
use sp_core::H256;
use sp_runtime::app_crypto::UncheckedFrom;
use sp_runtime::AccountId32;

impl GearRuntimeTestCmd {
    /// Runs tests from `.yaml` files using the Gear pallet for interaction.
    pub fn run(&self, _config: Configuration) -> sc_cli::Result<()> {
        let mut tests = vec![];
        for path in &self.input {
            if path.is_dir() {
                for entry in path.read_dir().expect("read_dir call failed").flatten() {
                    tests.push(read_test_from_file(entry.path()).map_err(|e| e.to_string())?);
                }
            } else {
                tests.push(read_test_from_file(path).map_err(|e| e.to_string())?);
            }
        }

        let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
        let total_failed = AtomicUsize::new(0);

        println!("Total fixtures: {}", total_fixtures);
        tests.par_iter().for_each(|test| {
            test.fixtures.par_iter().for_each(|fixture| {
                new_test_ext().execute_with(|| {
                    gear_common::reset_storage();
                    let output = run_fixture(test, fixture);
                    pallet_gear::Mailbox::<Runtime>::drain();

                    println!("Fixture {}: {}", fixture.title.bold(), output);
                    if !output.contains("Ok") && !output.contains("Skip") {
                        total_failed.fetch_add(1, Ordering::SeqCst);
                    }
                });
            });
        });
        if total_failed.load(Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err(format!("{} tests failed", total_failed.load(Ordering::SeqCst)).into())
        }
    }
}

impl CliConfiguration for GearRuntimeTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}

fn init_fixture(
    test: &'_ sample::Test,
    snapshots: &mut Vec<DebugData>,
    mailbox: &mut Vec<StoredMessage>,
) -> anyhow::Result<()> {
    if let Some(codes) = &test.codes {
        for code in codes {
            let code_bytes = std::fs::read(&code.path).map_err(|e| {
                anyhow::format_err!(
                    "Tried to read code from path {:?}. Failed: {:?}",
                    code.path,
                    e
                )
            })?;

            if let Err(e) = GearPallet::<Runtime>::submit_code(
                Origin::from(Some(AccountId32::unchecked_from(1000001.into_origin()))),
                code_bytes,
            ) {
                return Err(anyhow::format_err!("Submit code error: {:?}", e));
            }
        }
    }

    for program in &test.programs {
        let program_path = program.path.clone();
        let code = std::fs::read(&program_path).unwrap();
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => parse_payload(s.clone()).into_bytes(),
                PayloadVariant::Custom(v) => {
                    let meta_type = MetaType::InitInput;

                    let payload =
                        parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                    let json = MetaData::Json(payload);

                    let wasm = gear_test::sample::get_meta_wasm_path(program_path);

                    json.convert(&wasm, &meta_type)
                        .expect("Unable to get bytes")
                        .into_bytes()
                }
                _ => init_msg.clone().into_raw(),
            }
        }

        if let Err(e) = GearPallet::<Runtime>::submit_program(
            Origin::from(Some(AccountId32::unchecked_from(1000001.into_origin()))),
            code.clone(),
            program.id.to_program_id().as_ref().to_vec(),
            init_message,
            program.init_gas_limit.unwrap_or(5_000_000_000),
            program.init_value.unwrap_or(0) as u128,
        ) {
            return Err(anyhow::format_err!("Submit program error: {:?}", e));
        }

        // Initialize programs
        process_queue(snapshots, mailbox);
    }

    Ok(())
}

fn run_fixture(test: &'_ sample::Test, fixture: &sample::Fixture) -> ColoredString {
    let mut snapshots = Vec::new();
    let mut progs_n_paths: Vec<(&str, ProgramId)> = vec![];
    pallet_gear_debug::DebugMode::<Runtime>::put(true);

    // Find out future program ids
    let mut programs: BTreeMap<ProgramId, H256> = test
        .programs
        .iter()
        .map(|program| {
            let program_path = program.path.clone();
            let code = std::fs::read(&program_path).unwrap();
            let salt = program.id.to_program_id().as_ref().to_vec();

            let id = ProgramId::generate(CodeId::generate(&code), &salt);

            progs_n_paths.push((program.path.as_ref(), id));

            (program.id.to_program_id(), id.into_origin())
        })
        .collect();

    let programs_map: BTreeMap<H256, H256> = programs
        .iter()
        .map(|(k, v)| (H256::from_slice(k.as_ref()), *v))
        .collect();

    // Fill the key in the storage with a fake Program ID so that messages to this program get into the message queue
    for id in programs_map.keys() {
        let program = gear_common::ActiveProgram {
            static_pages: 0,
            persistent_pages: Default::default(),
            code_hash: H256::default(),
            state: gear_common::ProgramState::Initialized,
        };
        gear_common::set_program(*id, program, Default::default());
    }

    // Enable remapping of the source and destination of messages
    pallet_gear_debug::ProgramsMap::<Runtime>::put(programs_map);
    pallet_gear_debug::RemapId::<Runtime>::put(true);
    let mut mailbox: Vec<StoredMessage> = vec![];

    match init_fixture(test, &mut snapshots, &mut mailbox) {
        Ok(()) => {
            log::trace!("programs: {:?}", &programs);
            let mut errors = vec![];
            let mut expected_log = vec![];

            for message in &fixture.messages {
                let payload = match &message.payload {
                    Some(PayloadVariant::Utf8(s)) => parse_payload(s.clone()).as_bytes().to_vec(),
                    Some(PayloadVariant::Custom(v)) => {
                        let meta_type = MetaType::HandleInput;

                        let payload = parse_payload(
                            serde_json::to_string(&v).expect("Cannot convert to string"),
                        );

                        let json = MetaData::Json(payload);

                        let wasm = gear_test::sample::get_meta_wasm_path(
                            test.programs
                                .iter()
                                .filter(|p| p.id == message.destination)
                                .last()
                                .expect("Program not found")
                                .path
                                .clone(),
                        );
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

                let dest = programs[&message.destination.to_program_id()];

                let gas_limit = message.gas_limit.unwrap_or(
                    GearPallet::<Runtime>::gas_allowance() / fixture.messages.len() as u64,
                );

                let value = message.value.unwrap_or(0);

                // Force push to MQ if msg.source.is_some()
                if let Some(source) = &message.source {
                    let source = H256::from_slice(source.to_program_id().as_ref());
                    let id = GearPallet::<Runtime>::next_message_id(source);

                    let _ =
                        <Runtime as pallet_gear::Config>::GasHandler::create(source, id, gas_limit);

                    let msg = StoredMessage::new(
                        MessageId::from_origin(id),
                        ProgramId::from_origin(source),
                        ProgramId::from_origin(dest),
                        payload,
                        value,
                        None,
                    );
                    gear_common::queue_dispatch(StoredDispatch::new(
                        DispatchKind::Handle,
                        msg,
                        None,
                    ));
                } else {
                    GearPallet::<Runtime>::send_message(
                        Origin::from(Some(AccountId32::unchecked_from(1000001.into_origin()))),
                        dest,
                        payload,
                        gas_limit,
                        value,
                    );
                }

                // After initialization the last snapshot is empty, so we get MQ after sending messages
                snapshots.last_mut().unwrap().dispatch_queue = get_dispatch_queue();
            }

            process_queue(&mut snapshots, &mut mailbox);

            // After processing queue some new programs could be created, so we
            // search for them
            for snapshot_program in &snapshots.last().unwrap().programs {
                let exists = programs.iter().any(|(k, v)| {
                    k.into_origin() == snapshot_program.id || v == &snapshot_program.id
                });
                if exists {
                    continue;
                } else {
                    // A new program was created
                    programs.insert(
                        ProgramId::from_origin(snapshot_program.id),
                        snapshot_program.id,
                    );
                }
            }

            for exp in &fixture.expected {
                let snapshot: DebugData = if let Some(step) = exp.step {
                    if snapshots.len() < (step + test.programs.len()) {
                        Default::default()
                    } else {
                        snapshots[(step + test.programs.len()) - 1].clone()
                    }
                } else {
                    snapshots.last().unwrap().clone()
                };

                let mut message_queue: Vec<(StoredMessage, GasLimit)> = snapshot
                    .dispatch_queue
                    .iter()
                    .map(|dispatch| (dispatch.message().clone(), 0))
                    .collect();

                if let Some(mut expected_messages) = exp.messages.clone() {
                    if expected_messages.is_empty() {
                        break;
                    }

                    message_queue.iter_mut().for_each(|(msg, _gas)| {
                        if let Some(id) = programs.get(&msg.destination()) {
                            *msg = StoredMessage::new(
                                msg.id(),
                                msg.source(),
                                ProgramId::from(id.as_bytes()),
                                msg.payload().to_vec(),
                                msg.value(),
                                msg.reply(),
                            );
                        }
                    });

                    expected_messages.iter_mut().for_each(|msg| {
                        msg.destination = gear_test::address::Address::H256(
                            programs[&msg.destination.to_program_id()],
                        )
                    });

                    // For runtime tests gas check skipped due to absence of gas tree in snapshot.
                    if let Err(msg_errors) = gear_test::check::check_messages(
                        &progs_n_paths,
                        &message_queue,
                        &expected_messages,
                        true,
                    ) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(
                            msg_errors
                                .into_iter()
                                .map(|err| format!("Messages check [{}]", err)),
                        );
                    }
                }
                let mut progs: Vec<gear_core::program::Program> = snapshot
                    .programs
                    .iter()
                    .filter_map(|p| {
                        if let Some((pid, _)) = programs.iter().find(|(_, v)| v == &&p.id) {
                            let code = gear_common::get_code(p.code_hash)
                                .expect("code should be in the storage");
                            Some(CoreProgram::from_parts(
                                *pid,
                                code,
                                p.persistent_pages.keys().cloned().collect(),
                                true,
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                if let Some(expected_memory) = &exp.memory {
                    if let Err(mem_errors) =
                        gear_test::check::check_memory(&mut progs, expected_memory)
                    {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(mem_errors);
                    }
                }

                if let Some(alloc) = &exp.allocations {
                    if let Err(alloc_errors) = gear_test::check::check_allocations(&progs, alloc) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(alloc_errors);
                    }
                }

                if let Some(log) = &exp.log {
                    expected_log.append(&mut log.clone());
                }
            }

            if !expected_log.is_empty() {
                log::trace!("mailbox: {:?}", &mailbox);

                let messages: Vec<(StoredMessage, GasLimit)> =
                    mailbox.into_iter().map(|msg| (msg, 0)).collect();

                for (message, _) in &messages {
                    if let Ok(utf8) = core::str::from_utf8(message.payload()) {
                        log::trace!("log({})", utf8)
                    }
                }

                if let Err(log_errors) =
                    gear_test::check::check_messages(&progs_n_paths, &messages, &expected_log, true)
                {
                    errors.extend(
                        log_errors
                            .into_iter()
                            .map(|err| format!("Log check [{}]", err)),
                    );
                }
            }

            if !errors.is_empty() {
                errors.insert(0, "\n".to_string());
                errors.join("\n").bright_red()
            } else {
                "Ok".bright_green()
            }
        }
        Err(e) => format!("Initialization error ({})", e).bright_red(),
    }
}
