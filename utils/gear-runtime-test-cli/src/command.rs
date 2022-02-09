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

use std::collections::BTreeMap;

use crate::mock::{new_test_ext, run_to_block, System, Test, USER_1};
use crate::GearRuntimeTestCmd;
use codec::Encode;
use colored::{ColoredString, Colorize};

use gear_core::{message::Message, program::ProgramId};

use gear_test::{
    check::read_test_from_file,
    js::{MetaData, MetaType},
    proc::*,
    sample,
    sample::PayloadVariant,
};
use pallet_gear::Pallet as GearPallet;
use pallet_gear_debug::DebugData;
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;
use sp_core::H256;

fn init_fixture(
    test: &'_ sample::Test,
    _fixture: &sample::Fixture,
    programs: &BTreeMap<ProgramId, H256>,
    _progs_n_paths: &mut [(&'_ str, ProgramId)],
    snapshots: &mut Vec<DebugData>,
) -> anyhow::Result<()> {
    for program in &test.programs {
        log::info!("programs: {:?}", &programs);
        // match program.id {
        //     Address::ProgramId(_) => (),
        //     _ => return Err(anyhow::anyhow!("Program custom id - Skip")),
        // }
        let program_path = program.path.clone();
        let code = std::fs::read(&program_path).unwrap();
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => parse_payload(s.clone(), Some(programs)).into_bytes(),
                PayloadVariant::Custom(v) => {
                    let meta_type = MetaType::InitInput;

                    let payload = parse_payload(
                        serde_json::to_string(&v).expect("Cannot convert to string"),
                        Some(programs),
                    );

                    let json = MetaData::Json(payload);

                    let wasm = program_path.replace(".wasm", ".meta.wasm");

                    json.convert(&wasm, &meta_type)
                        .expect("Unable to get bytes")
                        .into_bytes()
                }
                _ => init_msg.clone().into_raw(),
            }
        }

        // let message_id = MessageId::from(nonce);
        // let id = program.id.to_program_id();

        // println!("init: {:?}", init_message);
        if let Err(e) = GearPallet::<Test>::submit_program(
            crate::mock::Origin::signed(USER_1),
            code.clone(),
            program.id.to_program_id().as_slice().to_vec(),
            init_message,
            program.init_gas_limit.unwrap_or(5_000_000_000),
            program.init_value.unwrap_or(0) as u128,
        ) {
            return Err(anyhow::format_err!("Init fixture err: {:?}", e));
        }
        while !gear_common::StorageQueue::<gear_common::Message>::get(
            gear_common::STORAGE_MESSAGE_PREFIX,
        )
        .is_empty()
        {
            run_to_block(System::block_number() + 1, None);
            let events = System::events();
            for event in events {
                if let crate::mock::Event::GearDebug(
                    pallet_gear_debug::Event::DebugDataSnapshot(snapshot),
                ) = &event.event
                {
                    // snapshots.push(snapshot);
                    snapshots.push(snapshot.clone());
                }
            }
            System::reset_events();
        }
    }

    Ok(())
}

fn run_fixture(test: &'_ sample::Test, fixture: &sample::Fixture) -> ColoredString {
    let mut snapshots = Vec::new();
    let mut progs_n_paths: Vec<(&str, ProgramId)> = vec![];
    pallet_gear_debug::DebugMode::<Test>::put(true);
    let programs: BTreeMap<ProgramId, H256> = test
        .programs
        .iter()
        .map(|program| {
            let program_path = program.path.clone();
            let code = std::fs::read(&program_path).unwrap();

            let salt = program.id.to_program_id().as_slice().to_vec();
            let mut data = Vec::new();
            // TODO #512
            code.encode_to(&mut data);
            salt.encode_to(&mut data);

            // Make sure there is no program with such id in program storage
            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

            progs_n_paths.push((program.path.as_ref(), ProgramId::from(id.as_bytes())));

            (program.id.to_program_id(), id)
        })
        .collect();

    let programs_map: BTreeMap<H256, H256> = programs
        .iter()
        .map(|(k, v)| (H256::from_slice(k.as_slice()), *v))
        .collect();

    for id in programs_map.keys() {
        sp_io::storage::set(&gear_common::program_key(*id), &[]);
    }

    pallet_gear_debug::ProgramsMap::<Test>::put(programs_map);
    pallet_gear_debug::RemapId::<Test>::put(true);

    match init_fixture(test, fixture, &programs, &mut progs_n_paths, &mut snapshots) {
        Ok(()) => {
            let mut errors = vec![];
            let mut expected_log = vec![];
            let mut mailbox: Vec<gear_common::Message> = vec![];
            pallet_gear::Mailbox::<Test>::drain().for_each(|(_, user_mailbox)| {
                for msg in user_mailbox.values() {
                    mailbox.push(msg.clone())
                }
            });

            for message in &fixture.messages {
                // Set custom source
                let payload = match &message.payload {
                    Some(PayloadVariant::Utf8(s)) => parse_payload(s.clone(), Some(&programs))
                        .as_bytes()
                        .to_vec(),
                    Some(PayloadVariant::Custom(v)) => {
                        let meta_type = MetaType::HandleInput;

                        let payload = parse_payload(
                            serde_json::to_string(&v).expect("Cannot convert to string"),
                            Some(&programs),
                        );

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

                let gas_limit = message
                    .gas_limit
                    .unwrap_or(GearPallet::<Test>::gas_allowance() / fixture.messages.len() as u64);

                // TODO: force queue message if custom source
                // if let Some(source) = &message.source {
                //     if programs.contains_key(&source.to_program_id()) {
                //         let msg = gear_common::Message {
                //             id: todo!(),
                //             source: todo!(),
                //             dest: todo!(),
                //             payload,
                //             gas_limit,
                //             value: todo!(),
                //             reply: todo!(),
                //         }
                //         gear_common::queue_message(message)
                //     }
                // } else {
                //     USER_1
                // };
                if message.source.is_some() {
                    return "Message custom source - Skip".bright_yellow();
                }

                log::info!(
                    "{:?}",
                    GearPallet::<Test>::send_message(
                        crate::mock::Origin::signed(USER_1),
                        programs[&message.destination.to_program_id()],
                        payload,
                        gas_limit,
                        message.value.unwrap_or(0),
                    )
                );
            }

            while !gear_common::StorageQueue::<gear_common::Message>::get(
                gear_common::STORAGE_MESSAGE_PREFIX,
            )
            .is_empty()
            {
                // println!("strage queue: {:?", sp_io::storage::get());
                run_to_block(System::block_number() + 1, None);
                let events = System::events();
                for event in events {
                    if let crate::mock::Event::GearDebug(
                        pallet_gear_debug::Event::DebugDataSnapshot(snapshot),
                    ) = &event.event
                    {
                        // snapshots.push(snapshot);
                        snapshots.push(snapshot.clone());
                    }
                }
                System::reset_events();
                pallet_gear::Mailbox::<Test>::drain().for_each(|(_, user_mailbox)| {
                    for msg in user_mailbox.values() {
                        mailbox.push(msg.clone())
                    }
                });
            }

            for exp in &fixture.expected {
                if let Some(mut expected_messages) = exp.messages.clone() {
                    if expected_messages.is_empty() {
                        break;
                    }
                    let mut message_queue: Vec<Message> = if let Some(step) = exp.step {
                        println!(
                            "snapshots.len() = {}, step({}), progs({})",
                            snapshots.len(),
                            step,
                            test.programs.len()
                        );
                        if step == 0 {
                            continue;
                        }
                        snapshots
                            .get((step + test.programs.len()) - 1)
                            .unwrap()
                            .message_queue
                            .iter()
                            .map(|msg| Message::from(msg.clone()))
                            .collect()
                    } else {
                        snapshots
                            .last()
                            .unwrap()
                            .message_queue
                            .iter()
                            .map(|msg| Message::from(msg.clone()))
                            .collect()
                    };

                    message_queue.iter_mut().for_each(|msg| {
                        if let Some(id) = programs.get(&msg.dest) {
                            msg.dest = ProgramId::from(id.as_bytes());
                        }
                    });

                    expected_messages.iter_mut().for_each(|msg| {
                        msg.destination = gear_test::address::Address::H256(
                            programs[&msg.destination.to_program_id()],
                        )
                    });

                    if let Err(msg_errors) = gear_test::check::check_messages(
                        &progs_n_paths,
                        &message_queue,
                        &expected_messages,
                        Some(&programs),
                    ) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(
                            msg_errors
                                .into_iter()
                                .map(|err| format!("Messages check [{}]", err)),
                        );
                    }

                    // println!("res: {:#?}", &message_queue);
                    // println!("res: {:#?}", res);
                }
                // let user_id = &<T::AccountId as Origin>::from_origin(USER_1);
                if let Some(log) = &exp.log {
                    expected_log.append(&mut log.clone());
                }
            }
            if !expected_log.is_empty() {
                log::info!("Some(mailbox): {:?}", &mailbox);

                let messages: Vec<Message> = mailbox
                    .iter()
                    .map(|msg| Message::from(msg.clone()))
                    .collect();

                for message in &messages {
                    if let Ok(utf8) = core::str::from_utf8(message.payload()) {
                        println!("log({})", utf8)
                    }
                }

                if let Err(log_errors) = gear_test::check::check_messages(
                    &progs_n_paths,
                    &messages,
                    &expected_log,
                    Some(&programs),
                ) {
                    errors.extend(
                        log_errors
                            .into_iter()
                            .map(|err| format!("Log check [{}]", err)),
                    );
                }
            }

            if !errors.is_empty() {
                errors.insert(0, "\n".to_string());
                // total_failed.fetch_add(1, Ordering::SeqCst);
                errors.join("\n").bright_red()
            } else {
                "Ok".bright_green()
            }
        }
        Err(e) => {
            // total_failed += 1;
            format!("Initialization error ({})", e).bright_red()
        }
    }
}

impl GearRuntimeTestCmd {
    /// Runs tests from `.yaml` files.
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
        log::info!("tests: {:?}", tests.len());

        let total_fixtures: usize = tests.iter().map(|t| t.fixtures.len()).sum();
        let mut total_failed = 0;

        println!("Total fixtures: {}", total_fixtures);

        for test in &tests {
            // let test = read_test_from_file(input).map_err(|e| e.to_string())?;
            // println!("Test {:?}", test.ti);

            for fixture in &test.fixtures {
                new_test_ext().execute_with(|| {
                    let output = run_fixture(test, fixture);
                    gear_common::reset_storage();
                    pallet_gear::Mailbox::<Test>::drain();

                    println!("Fixture {}: {}", fixture.title.bold(), output);
                    if !output.contains("Ok") && !output.contains("Skip") {
                        total_failed += 1;
                    }
                });
            }
        }
        if total_failed == 0 {
            return Ok(());
        } else {
            return Err(format!("{} tests failed", total_failed).into());
        }
        Ok(())
    }
}

impl CliConfiguration for GearRuntimeTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}
