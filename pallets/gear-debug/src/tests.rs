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
use frame_support::assert_ok;
use frame_system::{Pallet as SystemPallet};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::{DispatchKind, StoredDispatch, StoredMessage},
};
use pallet_gear::{DebugInfo, Pallet as PalletGear};
use sp_core::H256;
use sp_std::collections::btree_map::BTreeMap;

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
            10_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        Pallet::<Test>::do_snapshot();

        let static_pages = WasmPageNumber(16);

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id_1,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages: Default::default(),
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
            10_000_000_000_u64,
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
                            persistent_pages: Default::default(),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
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
            1_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_1 = get_last_message_id();

        PalletGear::<Test>::send_message(
            Origin::signed(1),
            program_id_2,
            vec![],
            1_000_000_000_u64,
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
                            persistent_pages: Default::default(),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
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
                            persistent_pages: Default::default(),
                            code_hash: generate_code_hash(&code_2),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
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
    use pallet_gear::Event;

    let event = match SystemPallet::<Test>::events()
        .last()
        .map(|r| r.event.clone())
    {
        Some(super::mock::Event::Gear(e)) => e,
        _ => unreachable!("Should be one Gear event"),
    };

    match event {
        Event::MessageEnqueued { id, .. } => id,
        Event::UserMessageSent { message, .. } => message.id(),
        _ => unreachable!("expect sending"),
    }
}

#[test]
fn check_not_allocated_pages() {
    // Currently we has no mechanism to restrict not allocated pages access during wasm execution
    // (this is true only for pages, which is laying inside allocated wasm memory,
    //  but which is not marked as allocated for program)
    // So, the test checks, that these pages can be used during execution,
    // but wont' be updated or uploaded to storage after execution.
    let wat = r#"
        (module
            (import "env" "memory" (memory 0))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (import "env" "free" (func $free (param i32)))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $init
                (local $i i32)

                ;; alloc 8 pages, so mem pages are: 0..=7
                (block
                    i32.const 8
                    call $alloc
                    i32.eqz
                    br_if 0
                    unreachable
                )

                ;; free all pages between 0 and 7
                (loop
                    local.get $i
                    i32.const 1
                    i32.add
                    local.set $i

                    local.get $i
                    call $free

                    local.get $i
                    i32.const 6
                    i32.ne
                    br_if 0
                )

                ;; write data in all pages, even in free one
                i32.const 0
                local.set $i
                (loop
                    local.get $i
                    i32.const 0x10000
                    i32.mul
                    i32.const 0x42
                    i32.store

                    local.get $i
                    i32.const 1
                    i32.add
                    local.set $i

                    local.get $i
                    i32.const 8
                    i32.ne
                    br_if 0
                )
            )
            (func $handle
                (local $i i32)

                ;; checks that all not allocated pages (0..=6) has zero values
                ;; !!! currently we can use not allocated pages during execution
                (loop
                    local.get $i
                    i32.const 1
                    i32.add
                    local.set $i

                    (block
                        local.get $i
                        i32.const 0x10000
                        i32.mul
                        i32.load
                        i32.eqz
                        br_if 0
                        unreachable
                    )

                    local.get $i
                    i32.const 6
                    i32.ne
                    br_if 0
                )

                ;; page 1 is allocated, so must have value, which we set in init
                (block
                    i32.const 0
                    i32.load
                    i32.const 0x42
                    i32.eq
                    br_if 0
                    unreachable
                )

                ;; page 7 is allocated, so must have value, which we set in init
                (block
                    i32.const 0x70000
                    i32.load
                    i32.const 0x42
                    i32.eq
                    br_if 0
                    unreachable
                )

                ;; store 1 to the begin of memomry to identify that test goes right
                i32.const 0
                i32.const 1
                i32.store
            )
        )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = generate_program_id(&code);
        let origin = Origin::signed(1);

        assert_ok!(PalletGear::<Test>::submit_program(
            origin.clone(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
        ));

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        Pallet::<Test>::do_snapshot();

        let gear_page0 = PageNumber::new_from_addr(0);
        let mut page0_data = PageBuf::new_zeroed();
        page0_data[0] = 0x42;

        let gear_page7 = PageNumber::new_from_addr(0x70000);
        let mut page7_data = PageBuf::new_zeroed();
        page7_data[0] = 0x42;

        let mut persistent_pages = BTreeMap::new();
        persistent_pages.insert(gear_page0, page0_data.to_vec());
        persistent_pages.insert(gear_page7, page7_data.to_vec());

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages: 0.into(),
                        persistent_pages: persistent_pages.clone(),
                        code_hash: generate_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );

        assert_ok!(PalletGear::<Test>::send_message(
            origin,
            program_id,
            vec![],
            5_000_000_000_u64,
            0_u128
        ));

        run_to_block(3, None);

        Pallet::<Test>::do_snapshot();

        page0_data[0] = 0x1;
        persistent_pages.insert(gear_page0, page0_data.to_vec());

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages: 0.into(),
                        persistent_pages: persistent_pages.clone(),
                        code_hash: generate_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );
    })
}

#[test]
fn check_changed_pages_in_storage() {
    // This test checks that only pages with changed data will be stored in storage.
    // Also it checks that data in storage is correct.
    // This test must works correct both with lazy pages and without it.
    let wat = r#"
        (module
            (import "env" "memory" (memory 8))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (import "env" "free" (func $free (param i32)))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $init
                ;; alloc 4 pages, so mem pages are: 0..=11
                (block
                    i32.const 4
                    call $alloc
                    i32.const 8
                    i32.eq
                    br_if 0
                    unreachable
                )

                ;; access page 1 (static)
                i32.const 0x10009  ;; is symbol "9" address
                i32.const 0x30     ;; is "0"
                i32.store

                ;; access page 7 (static) but do not change it
                (block
                    i32.const 0x70001
                    i32.load
                    i32.const 0x52414547 ;; is "GEAR"
                    i32.eq
                    br_if 0
                    unreachable
                )

                ;; access page 8 (dynamic)
                i32.const 0x87654
                i32.const 0x42
                i32.store

                ;; then free page 8
                i32.const 8
                call $free

                ;; then alloc page 8 again
                (block
                    i32.const 1
                    call $alloc
                    i32.const 8
                    i32.eq
                    br_if 0
                    unreachable
                )

                ;; access page 9 (dynamic)
                i32.const 0x98765
                i32.const 0x42
                i32.store

                ;; access page 10 (dynamic) but do not change it
                (block
                    i32.const 0xa9876
                    i32.load
                    i32.eqz             ;; must be zero by default
                    br_if 0
                    unreachable
                )

                ;; access page 11 (dynamic)
                i32.const 0xb8765
                i32.const 0x42
                i32.store

                ;; then free page 11
                i32.const 11
                call $free
            )

            (func $handle
                (block
                    ;; check page 1 data
                    i32.const 0x10002
                    i64.load
                    i64.const 0x3038373635343332  ;; is "23456780", "0" because we change it in init
                    i64.eq
                    br_if 0
                    unreachable
                )
                (block
                    ;; check page 7 data
                    i32.const 0x70001
                    i32.load
                    i32.const 0x52414547 ;; is "GEAR"
                    i32.eq
                    br_if 0
                    unreachable
                )
                (block
                    ;; check page 8 data
                    ;; currently free + allocation must save page data,
                    ;; but this behavior may change in future.
                    i32.const 0x87654
                    i32.load
                    i32.const 0x42
                    i32.eq
                    br_if 0
                    unreachable
                )
                (block
                    ;; check page 9 data
                    i32.const 0x98765
                    i32.load
                    i32.const 0x42
                    i32.eq
                    br_if 0
                    unreachable
                )
                ;; change page 3 and 4
                ;; because we store 0x00_00_00_42 then bits will be changed
                ;; in 3th page only, so the 3th page only must be in storage.
                i32.const 0x3fffd
                i32.const 0x42
                i32.store
            )

            (data $.rodata (i32.const 0x10000) "0123456789")
            (data $.rodata (i32.const 0x70001) "GEAR TECH")
        )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = generate_program_id(&code);
        let origin = Origin::signed(1);

        assert_ok!(PalletGear::<Test>::submit_program(
            origin.clone(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
        ));

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        Pallet::<Test>::do_snapshot();

        let static_pages = WasmPageNumber(8);

        let page1_addr = 0x10000;
        let gear_page1 = PageNumber::new_from_addr(page1_addr);
        let mut page1_data = PageBuf::new_zeroed();
        page1_data[..10].copy_from_slice(b"0123456780".as_slice());

        let page8_addr = 0x87654;
        let gear_page8 = PageNumber::new_from_addr(page8_addr);
        let mut page8_data = PageBuf::new_zeroed();
        page8_data[page8_addr % PageNumber::size()] = 0x42;

        let page9_addr = 0x98765;
        let gear_page9 = PageNumber::new_from_addr(page9_addr);
        let mut page9_data = PageBuf::new_zeroed();
        page9_data[page9_addr % PageNumber::size()] = 0x42;

        let mut persistent_pages = BTreeMap::new();
        persistent_pages.insert(gear_page1, page1_data.to_vec());
        persistent_pages.insert(gear_page8, page8_data.to_vec());
        persistent_pages.insert(gear_page9, page9_data.to_vec());

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages: persistent_pages.clone(),
                        code_hash: generate_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );

        assert_ok!(PalletGear::<Test>::send_message(
            origin,
            program_id,
            vec![],
            5_000_000_000_u64,
            0_u128
        ));

        run_to_block(3, None);

        Pallet::<Test>::do_snapshot();

        let page3_addr = 0x3fffd;
        let gear_page3 = PageNumber::new_from_addr(page3_addr);
        let mut page3_data = PageBuf::new_zeroed();
        page3_data[page3_addr % PageNumber::size()] = 0x42;

        persistent_pages.insert(gear_page3, page3_data.to_vec());

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages,
                        code_hash: generate_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );
    })
}
