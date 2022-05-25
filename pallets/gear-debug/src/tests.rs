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

use super::*;
use crate::mock::*;
use common::{self, Origin as _};
use core::convert::TryInto;
use frame_system::Pallet as SystemPallet;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::WasmPageNumber,
    message::{DispatchKind, StoredDispatch, StoredMessage},
};
use pallet_gear::{DebugInfo, Pallet as PalletGear};
use sp_core::H256;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

fn parse_wat(source: &str) -> Vec<u8> {
    wabt::Wat2Wasm::new()
        .validate(false)
        .convert(source)
        .expect("failed to parse module")
        .as_ref()
        .to_vec()
}

fn generate_program_id(code: &[u8]) -> ProgramId {
    ProgramId::generate(CodeId::generate(code), b"salt")
}

fn generate_code_hash(code: &[u8]) -> H256 {
    CodeId::generate(code).into_origin()
}

#[test]
fn debug_mode_works() {
    let wat_1 = r#"
        (module
            (import "env" "memory" (memory 16))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $init)
            (func $handle)
        )"#;

    let wat_2 = r#"
        (module
            (import "env" "memory" (memory 16))
            (import "env" "alloc"  (func $alloc (param i32) (result i32)))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $handle
              (local $pages_offset i32)
              (local.set $pages_offset (call $alloc (i32.const 4)))
            )
            (func $init)
        )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code_1 = parse_wat(wat_1);
        let code_2 = parse_wat(wat_2);

        let program_id_1 = generate_program_id(&code_1);
        let program_id_2 = generate_program_id(&code_2);

        PalletGear::<Test>::submit_program(
            Origin::signed(1),
            code_1.clone(),
            b"salt".to_vec(),
            Vec::new(),
            100_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        Pallet::<Test>::do_snapshot();

        let static_pages = WasmPageNumber(16);

        let pages = |prog_id: ProgramId| {
            if cfg!(feature = "lazy-pages") {
                Default::default()
            } else {
                let prog_id = prog_id.into_origin();
                let active_prog: common::ActiveProgram = common::get_program(prog_id)
                    .expect("Can't find program")
                    .try_into()
                    .expect("Program isn't active");

                common::get_program_data_for_pages(prog_id, active_prog.pages_with_data.iter())
                    .expect("Can't get data for pages")
                    .into_iter()
                    .map(|(k, v)| (k, v.to_vec()))
                    .collect()
            }
        };

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id_1,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages: pages(program_id_1),
                        code_hash: generate_code_hash(&code_1),
                    }),
                }],
            })
            .into(),
        );

        PalletGear::<Test>::submit_program(
            Origin::signed(1),
            code_2.clone(),
            b"salt".to_vec(),
            Vec::new(),
            100_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        run_to_block(3, None);

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_2),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_1),
                            code_hash: generate_code_hash(&code_1),
                        }),
                    },
                ],
            })
            .into(),
        );

        PalletGear::<Test>::send_message(
            Origin::signed(1),
            program_id_1,
            vec![],
            100_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_1 = get_last_message_id();

        PalletGear::<Test>::send_message(
            Origin::signed(1),
            program_id_2,
            vec![],
            100_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_2 = get_last_message_id();

        run_to_block(4, Some(0)); // no message will get processed

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![
                    StoredDispatch::new(
                        DispatchKind::Handle,
                        StoredMessage::new(
                            message_id_1,
                            1.into(),
                            program_id_1,
                            Default::default(),
                            0,
                            None,
                        ),
                        None,
                    ),
                    StoredDispatch::new(
                        DispatchKind::Handle,
                        StoredMessage::new(
                            message_id_2,
                            1.into(),
                            program_id_2,
                            Default::default(),
                            0,
                            None,
                        ),
                        None,
                    ),
                ],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_2),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_1),
                            code_hash: generate_code_hash(&code_1),
                        }),
                    },
                ],
            })
            .into(),
        );

        run_to_block(5, None); // no message will get processed
        Pallet::<Test>::do_snapshot();

        // only programs left!
        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_2),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: pages(program_id_1),
                            code_hash: generate_code_hash(&code_1),
                        }),
                    },
                ],
            })
            .into(),
        );
    })
}

fn get_last_message_id() -> MessageId {
    use pallet_gear::{Event, MessageInfo};

    let event = match SystemPallet::<Test>::events()
        .last()
        .map(|r| r.event.clone())
    {
        Some(super::mock::Event::Gear(e)) => e,
        _ => unreachable!("Should be one Gear event"),
    };

    match event {
        Event::InitMessageEnqueued(MessageInfo { message_id, .. }) => message_id,
        Event::Log(msg) => msg.id(),
        Event::DispatchMessageEnqueued(MessageInfo { message_id, .. }) => message_id,
        _ => unreachable!("expect sending"),
    }
}
