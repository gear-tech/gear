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

#![allow(unused_must_use, unused)]

use crate::{
    util::{MailboxOf, QueueOf},
    RuntimeTestCmd,
};
use colored::{ColoredString, Colorize};
use frame_support::traits::ReservableCurrency;
use gear_common::{storage::*, GasPrice, GasTree, Origin as _, ProgramStorage};
use gear_core::{
    ids::{CodeId, ProgramId},
    memory::PageU32Size,
    message::{DispatchKind, GasLimit, StoredDispatch, StoredMessage},
};
use gear_core_processor::common::ExecutableActorData;
use gear_test::{
    check::{read_test_from_file, ProgramAllocations},
    js::{MetaData, MetaType},
    proc::*,
    sample::{self, ChainProgram, PayloadVariant},
};
use junit_common::{TestCase, TestSuite, TestSuites};
use pallet_gear::{Config, GasAllowanceOf, GasHandlerOf, Pallet as GearPallet, ProgramStorageOf};
use pallet_gear_debug::{DebugData, ProgramState};
use quick_xml::Writer;
use rayon::prelude::*;
use sc_cli::{CliConfiguration, SharedParams};
use sp_core::H256;
use sp_runtime::{app_crypto::UncheckedFrom, AccountId32};
use std::{
    collections::BTreeMap,
    convert::TryInto,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

const DEFAULT_BLOCK_NUMBER: u32 = 0;

impl CliConfiguration for RuntimeTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}

#[cfg(any(feature = "gear-native", feature = "vara-native"))]
macro_rules! command {
    () => {
        pub(crate) fn run(param: &RuntimeTestCmd) -> sc_cli::Result<()> {
            let mut tests = vec![];
            for path in &param.input {
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
            let (executions, times) = tests
                .par_iter()
                .map(|test| {
                    let (fixtures, times) = test
                        .fixtures
                        .par_iter()
                        .map(|fixture| {
                            new_test_ext().execute_with(|| {
                                let now = Instant::now();
                                let output = run_fixture(test, fixture);
                                let elapsed = now.elapsed();

                                MailboxOf::<Runtime>::clear();

                                println!("Fixture {}: {}", fixture.title.bold(), output);
                                if !output.contains("Ok") && !output.contains("Skip") {
                                    total_failed.fetch_add(1, Ordering::SeqCst);
                                }

                                (
                                    TestCase {
                                        name: fixture.title.clone(),
                                        time: elapsed.as_secs_f64().to_string(),
                                    },
                                    elapsed.as_secs_f64(),
                                )
                            })
                        })
                        .collect::<(Vec<_>, Vec<_>)>();

                    (
                        TestSuite {
                            name: test.title.clone(),
                            testcase: fixtures,
                        },
                        times.iter().sum::<f64>(),
                    )
                })
                .collect::<(Vec<_>, Vec<_>)>();

            if let Some(ref junit_path) = param.generate_junit {
                let mut xml = String::new();
                quick_xml::se::to_writer(
                    &mut xml,
                    &TestSuites {
                        time: times.iter().sum::<f64>().to_string(),
                        testsuite: executions,
                    },
                )
                .map_err(|e| {
                    let mapped: Box<dyn std::error::Error + Send + Sync> = Box::new(e);
                    mapped
                })?;
                std::fs::write(junit_path, xml)?;
            }

            if total_failed.load(Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err(format!("{} tests failed", total_failed.load(Ordering::SeqCst)).into())
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

                    if let Err(e) = GearPallet::<Runtime>::upload_code(
                        RuntimeOrigin::from(Some(AccountId32::unchecked_from(
                            1000001.into_origin(),
                        ))),
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

                            let payload = parse_payload(
                                serde_json::to_string(&v).expect("Cannot convert to string"),
                            );

                            let json = MetaData::Json(payload);

                            let wasm = gear_test::sample::get_meta_wasm_path(program_path);

                            json.convert(&wasm, &meta_type)
                                .expect("Unable to get bytes")
                                .into_bytes()
                        }
                        _ => init_msg.clone().into_raw(),
                    }
                }

                if let Err(e) = GearPallet::<Runtime>::upload_program(
                    RuntimeOrigin::from(Some(AccountId32::unchecked_from(1000001.into_origin()))),
                    code.clone(),
                    program.id.to_program_id().as_ref().to_vec(),
                    init_message,
                    program.init_gas_limit.unwrap_or(50_000_000_000),
                    program.init_value.unwrap_or(0),
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
                    allocations: Default::default(),
                    pages_with_data: Default::default(),
                    code_hash: H256::default(),
                    code_exports: Default::default(),
                    static_pages: 0.into(),
                    state: gear_common::ProgramState::Initialized,
                    gas_reservation_map: Default::default(),
                };
                ProgramStorageOf::<Runtime>::add_program(
                    ProgramId::from_origin(*id),
                    program,
                    DEFAULT_BLOCK_NUMBER.into(),
                );
            }

            // Enable remapping of the source and destination of messages
            pallet_gear_debug::ProgramsMap::<Runtime>::put(programs_map);
            pallet_gear_debug::RemapId::<Runtime>::put(true);
            let mut mailbox: Vec<StoredMessage> = vec![];

            if let Err(err) = init_fixture(test, &mut snapshots, &mut mailbox) {
                return format!("Initialization error ({})", err).bright_red();
            }

            log::trace!("initial programs: {:?}", &programs);

            let mut errors = vec![];
            let mut expected_log = vec![];

            let empty = Vec::new();
            for message in fixture.messages.as_ref().unwrap_or(&empty) {
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

                let dest = ProgramId::from_origin(programs[&message.destination.to_program_id()]);

                let gas_limit = message
                    .gas_limit
                    .unwrap_or_else(GasAllowanceOf::<Runtime>::get);

                let value = message.value.unwrap_or(0);

                // Force push to MQ if msg.source.is_some()
                if let Some(source) = &message.source {
                    let source = H256::from_slice(source.to_program_id().as_ref());
                    let origin = <AccountId32 as gear_common::Origin>::from_origin(
                        crate::HACK.into_origin(),
                    );
                    let id = GearPallet::<Runtime>::next_message_id(source);

                    <Runtime as Config>::Currency::reserve(
                        &origin,
                        <Runtime as Config>::GasPrice::gas_price(gas_limit),
                    )
                    .expect("No more funds");

                    // # Safety
                    //
                    // This is unreachable since the `message_id` is new generated
                    // with `GearPallet::next_message_id`.
                    GasHandlerOf::<Runtime>::create(origin, id, gas_limit)
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    let msg = StoredMessage::new(
                        id,
                        ProgramId::from_origin(source),
                        dest,
                        payload.try_into().unwrap(),
                        value,
                        None,
                    );

                    QueueOf::<Runtime>::queue(StoredDispatch::new(DispatchKind::Handle, msg, None))
                        .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
                } else {
                    GearPallet::<Runtime>::send_message(
                        RuntimeOrigin::from(Some(AccountId32::unchecked_from(
                            1000001.into_origin(),
                        ))),
                        dest,
                        payload,
                        gas_limit,
                        value,
                    );
                }
                run_to_block(System::block_number() + 1, None, true);
            }

            // After initialization the last snapshot is empty, so we get MQ after sending messages
            snapshots.last_mut().unwrap().dispatch_queue = get_dispatch_queue();

            process_queue(&mut snapshots, &mut mailbox);

            let mut idx = 0;
            snapshots.iter().for_each(|s| {
                log::trace!(
                    "snapshot {} programs = {:?}",
                    idx,
                    s.programs.iter().map(|p| p.id).collect::<Vec<_>>()
                );
                idx += 1;
            });

            // After processing queue some new programs could be created, so we
            // search for them
            for snapshot_program in &snapshots.last().unwrap().programs {
                let exists = programs.iter().any(|(k, v)| {
                    *k == snapshot_program.id || ProgramId::from_origin(*v) == snapshot_program.id
                });
                if exists {
                    continue;
                } else {
                    // A new program was created
                    programs.insert(snapshot_program.id, snapshot_program.id.into_origin());
                }
            }

            let empty = Vec::new();
            for exp in fixture.expected.as_ref().unwrap_or(&empty) {
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
                    .into_iter()
                    .map(|dispatch| (dispatch.into_parts().1, 0))
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
                                msg.payload().to_vec().try_into().unwrap(),
                                msg.value(),
                                msg.details(),
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

                let actors_data: Vec<_> = snapshot
                    .programs
                    .iter()
                    .filter_map(|p| {
                        if let ProgramState::Active(info) = &p.state {
                            let (pid, _) = programs
                                .iter()
                                .find(|(_, &v)| ProgramId::from_origin(v) == p.id)?;
                            let code_id = CodeId::from_origin(info.code_hash);

                            let pages = info
                                .persistent_pages
                                .keys()
                                .copied()
                                .map(|p| p.to_page())
                                .collect();

                            let memory = info.persistent_pages.clone();
                            let gas_reservation_map = {
                                let (prog, _bn) =
                                    ProgramStorageOf::<Runtime>::get_program(*pid).unwrap();
                                if let gear_common::Program::Active(gear_common::ActiveProgram {
                                    gas_reservation_map,
                                    ..
                                }) = prog
                                {
                                    gas_reservation_map
                                } else {
                                    panic!("no gas reservation map found in program")
                                }
                            };
                            Some((
                                *pid,
                                ExecutableActorData {
                                    allocations: pages,
                                    code_id,
                                    code_exports: Default::default(),
                                    static_pages: info.static_pages,
                                    initialized: true,
                                    pages_with_data: memory.keys().cloned().collect(),
                                    gas_reservation_map,
                                },
                                memory,
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();

                if let Some(expected_memory) = &exp.memory {
                    if let Err(mem_errors) =
                        gear_test::check::check_memory(&actors_data, expected_memory)
                    {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(mem_errors);
                    }
                }

                if let Some(alloc) = &exp.allocations {
                    let allocations = actors_data
                        .iter()
                        .map(|(id, data, _)| ProgramAllocations {
                            id: *id,
                            static_pages: data.static_pages,
                            allocations: &data.allocations,
                        })
                        .collect::<Vec<_>>();
                    if let Err(alloc_errors) =
                        gear_test::check::check_allocations(&allocations, alloc)
                    {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(alloc_errors);
                    }
                }

                let actual_programs = snapshot
                    .programs
                    .iter()
                    .filter_map(|p| {
                        if let Some((pid, _)) = programs
                            .iter()
                            .find(|(_, &v)| ProgramId::from_origin(v) == p.id)
                        {
                            Some((*pid, p.state == ProgramState::Terminated))
                        } else {
                            None
                        }
                    })
                    .collect();

                if let Some(programs_struct) = &exp.programs {
                    let expected_programs = programs_struct
                        .ids
                        .iter()
                        .map(|program: &ChainProgram| {
                            (
                                program.address.to_program_id(),
                                program.terminated.unwrap_or_default(),
                            )
                        })
                        .collect();

                    if let Err(state_errors) = gear_test::check::check_programs_state(
                        &expected_programs,
                        &actual_programs,
                        programs_struct.only.unwrap_or_default(),
                    ) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(state_errors);
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
    };
}

#[cfg(feature = "gear-native")]
pub(crate) mod gear {
    use super::*;
    use crate::util::gear::{get_dispatch_queue, new_test_ext, process_queue, run_to_block};
    use gear_runtime::{Runtime, RuntimeOrigin, System};

    command!();
}

#[cfg(feature = "vara-native")]
pub(crate) mod vara {
    use super::*;
    use crate::util::vara::{get_dispatch_queue, new_test_ext, process_queue, run_to_block};
    use vara_runtime::{Runtime, RuntimeOrigin, System};

    command!();
}
