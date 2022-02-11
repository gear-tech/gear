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
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::mock::{new_test_ext, run_to_block, System, Test, USER_1};
use crate::GearRuntimeTestCmd;
use codec::{Decode, Encode};
use colored::{ColoredString, Colorize};

use gear_core::message::Message as CoreMessage;
use gear_core::program::ProgramId;

use gear_common::Message;
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
                    let output = run_fixture(test, fixture);
                    gear_common::reset_storage();
                    pallet_gear::Mailbox::<Test>::drain();

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
    mailbox: &mut Vec<Message>,
) -> anyhow::Result<()> {
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

                    let wasm = program_path.replace(".wasm", ".meta.wasm");

                    json.convert(&wasm, &meta_type)
                        .expect("Unable to get bytes")
                        .into_bytes()
                }
                _ => init_msg.clone().into_raw(),
            }
        }

        if let Err(e) = GearPallet::<Test>::submit_program(
            crate::mock::Origin::signed(USER_1),
            code.clone(),
            program.id.to_program_id().as_slice().to_vec(),
            init_message,
            program.init_gas_limit.unwrap_or(5_000_000_000),
            program.init_value.unwrap_or(0) as u128,
        ) {
            return Err(anyhow::format_err!("Submit program error: {:?}", e));
        }

        // Initialize programs
        while !gear_common::StorageQueue::<Message>::get(gear_common::STORAGE_MESSAGE_PREFIX)
            .is_empty()
        {
            run_to_block(System::block_number() + 1, None);
            // Parse data from events
            for event in System::events() {
                if let crate::mock::Event::GearDebug(pallet_gear_debug::Event::DebugDataSnapshot(
                    snapshot,
                )) = &event.event
                {
                    snapshots.push(snapshot.clone());
                }

                if let crate::mock::Event::Gear(pallet_gear::Event::Log(msg)) = &event.event {
                    mailbox.push(msg.clone());
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

    // Find out future program ids
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

            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

            progs_n_paths.push((program.path.as_ref(), ProgramId::from(id.as_bytes())));

            (program.id.to_program_id(), id)
        })
        .collect();

    let programs_map: BTreeMap<H256, H256> = programs
        .iter()
        .map(|(k, v)| (H256::from_slice(k.as_slice()), *v))
        .collect();

    // Fill the key in the storage with a fake Program ID so that messages to this program get into the message queue
    for id in programs_map.keys() {
        sp_io::storage::set(&gear_common::program_key(*id), &[]);
    }

    // Enable remapping of the source and destination of messages
    pallet_gear_debug::ProgramsMap::<Test>::put(programs_map);
    pallet_gear_debug::RemapId::<Test>::put(true);
    let mut mailbox: Vec<Message> = vec![];

    match init_fixture(test, &mut snapshots, &mut mailbox) {
        Ok(()) => {
            log::info!("programs: {:?}", &programs);
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
                //         let msg = Message {
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

                // After initialization the last snapshot is empty, so we get MQ after sending messages
                snapshots.last_mut().unwrap().message_queue = get_message_queue();
            }

            while !gear_common::StorageQueue::<Message>::get(gear_common::STORAGE_MESSAGE_PREFIX)
                .is_empty()
            {
                run_to_block(System::block_number() + 1, None);
                // Parse data from events
                for event in System::events() {
                    if let crate::mock::Event::GearDebug(
                        pallet_gear_debug::Event::DebugDataSnapshot(snapshot),
                    ) = &event.event
                    {
                        snapshots.push(snapshot.clone());
                    }

                    if let crate::mock::Event::Gear(pallet_gear::Event::Log(msg)) = &event.event {
                        mailbox.push(msg.clone());
                    }
                }
                System::reset_events();
            }

            for exp in &fixture.expected {
                if let Some(mut expected_messages) = exp.messages.clone() {
                    if expected_messages.is_empty() {
                        break;
                    }
                    let mut message_queue: Vec<CoreMessage> = if let Some(step) = exp.step {
                        snapshots
                            .get((step + test.programs.len()) - 1)
                            .unwrap()
                            .message_queue
                            .iter()
                            .map(|msg| CoreMessage::from(msg.clone()))
                            .collect()
                    } else {
                        snapshots
                            .last()
                            .unwrap()
                            .message_queue
                            .iter()
                            .map(|msg| CoreMessage::from(msg.clone()))
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
                    ) {
                        errors.push(format!("step: {:?}", exp.step));
                        errors.extend(
                            msg_errors
                                .into_iter()
                                .map(|err| format!("Messages check [{}]", err)),
                        );
                    }
                }

                if let Some(log) = &exp.log {
                    expected_log.append(&mut log.clone());
                }
            }

            if !expected_log.is_empty() {
                log::info!("mailbox: {:?}", &mailbox);

                let messages: Vec<CoreMessage> = mailbox
                    .iter()
                    .map(|msg| CoreMessage::from(msg.clone()))
                    .collect();

                for message in &messages {
                    if let Ok(utf8) = core::str::from_utf8(message.payload()) {
                        log::info!("log({})", utf8)
                    }
                }

                if let Err(log_errors) =
                    gear_test::check::check_messages(&progs_n_paths, &messages, &expected_log)
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
        Err(e) => {
            format!("Initialization error ({})", e).bright_red()
        }
    }
}

fn get_message_queue() -> Vec<Message> {
    #[derive(Decode, Encode)]
    struct Node {
        value: Message,
        next: Option<H256>,
    }

    let mq_head_key = [gear_common::STORAGE_MESSAGE_PREFIX, b"head"].concat();
    let mut message_queue = vec![];

    if let Some(head) = sp_io::storage::get(&mq_head_key) {
        let mut next_id = H256::from_slice(&head[..]);
        loop {
            let next_node_key = [gear_common::STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
            if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                let current_node = Node::decode(&mut &bytes[..]).unwrap();
                message_queue.push(current_node.value);
                match current_node.next {
                    Some(h) => next_id = h,
                    None => break,
                }
            }
        }
    }

    message_queue
}
