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

// use crate::manager::RuntestsExtManager;
use crate::mock::{
    new_test_ext, run_to_block, Event as MockEvent, Gear, System, Test, BLOCK_AUTHOR,
    LOW_BALANCE_USER, USER_1,
};
use crate::GearRuntimeTestCmd;
use codec::{Decode, Encode};
use gear_common::Origin as _;
use gear_core::{
    message::{IncomingMessage, Message},
    program::{Program, ProgramId},
};
use gear_core_processor::Ext;
use gear_runtime::Runtime;
use gear_test::check::check_messages;
use gear_test::{
    check::read_test_from_file,
    js::{MetaData, MetaType},
    proc::*,
    sample::PayloadVariant,
};
use pallet_gear::Pallet as GearPallet;
use pallet_gear_debug::Pallet as GearDebugPallet;
use rand::Rng;
use sc_cli::{CliConfiguration, SharedParams};
use sc_service::Configuration;
use sp_core::H256;
use sp_keyring::AccountKeyring;

impl GearRuntimeTestCmd {
    /// Runs tests from `.yaml` files.
    pub fn run(&self, _config: Configuration) -> sc_cli::Result<()> {
        for input in &self.input {
            let test = read_test_from_file(input).unwrap();
            log::info!("Test {:?}", input.file_name().unwrap());

            let mut progs_n_paths: Vec<(&str, ProgramId)> = vec![];

            for fixture in &test.fixtures {
                new_test_ext()
                    .execute_with(|| {
                        let mut errors = vec![];
                        let mut snapshots = Vec::new();
                        pallet_gear_debug::DebugMode::<Test>::put(true);
                        let mut programs = BTreeMap::new();

                        for program in &test.programs {
                            let program_path = program.path.clone();
                            let code = std::fs::read(&program_path)?;

                            let random_bytes = rand::thread_rng().gen::<[u8; 32]>().to_vec();
                            let mut data = Vec::new();
                            // TODO #512
                            code.encode_to(&mut data);
                            random_bytes.encode_to(&mut data);

                            // Make sure there is no program with such id in program storage
                            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

                            programs.insert(program.id.to_program_id(), id);
                            progs_n_paths
                                .push((program.path.as_ref(), ProgramId::from(id.as_bytes())));
                            let mut init_message = Vec::new();
                            if let Some(init_msg) = &program.init_message {
                                init_message = match init_msg {
                                    PayloadVariant::Utf8(s) => {
                                        parse_payload(s.clone()).into_bytes()
                                    }
                                    PayloadVariant::Custom(v) => {
                                        let meta_type = MetaType::InitInput;

                                        let payload = parse_payload(
                                            serde_json::to_string(&v)
                                                .expect("Cannot convert to string"),
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
                            let res = GearPallet::<Test>::submit_program(
                                crate::mock::Origin::signed(USER_1),
                                code.clone(),
                                random_bytes,
                                init_message,
                                program.init_gas_limit.unwrap_or(5_000_000_000),
                                program.init_value.unwrap_or(0) as u128,
                            );
                            // log::debug!("init extrinsic: {:?}", res);
                        }
                        log::info!("programs: {:?}", &programs);
                        for message in &fixture.messages {
                            let payload = match &message.payload {
                                Some(PayloadVariant::Utf8(s)) => {
                                    parse_payload(s.clone()).as_bytes().to_vec()
                                }
                                Some(PayloadVariant::Custom(v)) => {
                                    let meta_type = MetaType::HandleInput;

                                    let payload = parse_payload(
                                        serde_json::to_string(&v)
                                            .expect("Cannot convert to string"),
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

                            let gas_limit = message.gas_limit.unwrap_or(5_000_000_000);

                            match message.destination {
                                gear_test::address::Address::ProgramId(_) => {
                                    log::info!(
                                        "{:?}",
                                        GearPallet::<Test>::send_message(
                                            crate::mock::Origin::signed(USER_1).into(),
                                            programs[&message.destination.to_program_id()],
                                            payload,
                                            gas_limit, // `prog_id` program sends message in handle which sets gas limit to 10_000_000.
                                            message.value.unwrap_or(0),
                                        )
                                    );
                                }
                                _ => (),
                            }
                        }
                        let mut expected_log = vec![];
                        for exp in &fixture.expected {
                            while !gear_common::StorageQueue::<gear_common::Message>::get(
                                gear_common::STORAGE_MESSAGE_PREFIX,
                            )
                            .is_empty()
                            {
                                run_to_block(System::block_number() + 1, None);
                                let mut events = System::events();
                                for event in events {
                                    match &event.event {
                                        crate::mock::Event::GearDebug(snapshot) => {
                                            // snapshots.push(snapshot);
                                            match snapshot {
                                                pallet_gear_debug::Event::DebugDataSnapshot(
                                                    snapshot,
                                                ) => {
                                                    // println!("Got snapshot {:?}", snapshot);
                                                    snapshots.push(snapshot.clone());
                                                }
                                                _ => (),
                                            }
                                        }
                                        _ => (),
                                    }
                                }
                                // println!("snapshots: {:#?}", &snapshots);
                                // if sp_io::storage::get(
                                //     &[gear_common::STORAGE_MESSAGE_PREFIX, b"head"].concat(),
                                // )
                                // .is_none()
                                // {
                                //     break;
                                // }
                                run_to_block(System::block_number() + 1, None);
                                System::reset_events();
                            }

                            if let Some(mut expected_messages) = exp.messages.clone() {
                                let mut message_queue: Vec<Message> = if let Some(step) = exp.step {
                                    println!(
                                        "snapshots.len() = {}, step({}), progs({})",
                                        snapshots.len(),
                                        step,
                                        test.programs.len()
                                    );
                                    snapshots
                                        .get(step - 1)
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

                                // println!("res: {:#?}", &message_queue);
                                // println!("res: {:#?}", res);
                            }
                            // let user_id = &<T::AccountId as Origin>::from_origin(USER_1);
                            if let Some(log) = &exp.log {
                                expected_log.append(&mut log.clone());
                            }
                        }
                        let mailbox = GearPallet::<Test>::mailbox(USER_1);
                        log::info!("mailbox: {:?}", &mailbox);
                        if let Some(mailbox) = mailbox {
                            log::info!("Some(mailbox): {:?}", &mailbox);

                            let messages: Vec<Message> = mailbox
                                .values()
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
                            ) {
                                errors.extend(
                                    log_errors
                                        .into_iter()
                                        .map(|err| format!("Log check [{}]", err)),
                                );
                            }
                            // errors.push(res);
                        }

                        if !errors.is_empty() {
                            errors.insert(0, "\n".to_string());
                            // total_failed.fetch_add(1, Ordering::SeqCst);
                            println!("{}", errors.join("\n"));
                        } else {
                            println!("Ok");
                        }
                        gear_common::reset_storage();
                        pallet_gear::Mailbox::<Test>::drain();
                        Ok(())
                    })
                    .map_err(|e: anyhow::Error| sc_cli::Error::Application(e.into()))?;
            }
        }
        Ok(())
    }
}

impl CliConfiguration for GearRuntimeTestCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }
}
