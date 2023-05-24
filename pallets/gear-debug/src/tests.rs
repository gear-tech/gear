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
#[cfg(feature = "lazy-pages")]
use gear_core::memory::GearPage;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageU32Size, WasmPage},
    message::{DispatchKind, StoredDispatch, StoredMessage},
};
use gear_wasm_instrument::STACK_END_EXPORT_NAME;
use pallet_gear::{DebugInfo, Pallet as PalletGear};
use sp_core::H256;
use sp_std::collections::btree_map::BTreeMap;

const DEFAULT_SALT: &[u8] = b"salt";

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

fn parse_wat(source: &str) -> Vec<u8> {
    wabt::Wat2Wasm::new()
        .validate(true)
        .convert(source)
        .expect("failed to parse module")
        .as_ref()
        .to_vec()
}

fn h256_code_hash(code: &[u8]) -> H256 {
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
              i32.const 4
              call $alloc
              drop
            )
            (func $init)
        )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code_1 = parse_wat(wat_1);
        let code_2 = parse_wat(wat_2);

        let program_id_1 = ProgramId::generate(CodeId::generate(&code_1), DEFAULT_SALT);
        let program_id_2 = ProgramId::generate(CodeId::generate(&code_2), DEFAULT_SALT);

        PalletGear::<Test>::upload_program(
            RuntimeOrigin::signed(1),
            code_1.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            10_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        GearDebug::do_snapshot();

        let static_pages = 16.into();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id_1,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages: Default::default(),
                        code_hash: h256_code_hash(&code_1),
                    }),
                }],
            })
            .into(),
        );

        PalletGear::<Test>::upload_program(
            RuntimeOrigin::signed(1),
            code_2.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            10_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        run_to_block(3, None);

        GearDebug::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_1),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_2),
                        }),
                    },
                ],
            })
            .into(),
        );

        PalletGear::<Test>::send_message(
            RuntimeOrigin::signed(1),
            program_id_1,
            vec![],
            1_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_1 = get_last_message_id();

        PalletGear::<Test>::send_message(
            RuntimeOrigin::signed(1),
            program_id_2,
            vec![],
            1_000_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_2 = get_last_message_id();

        run_to_block(4, Some(0)); // no message will get processed

        GearDebug::do_snapshot();

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
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_1),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_2),
                        }),
                    },
                ],
            })
            .into(),
        );

        run_to_block(5, None); // no message will get processed
        GearDebug::do_snapshot();

        // only programs left!
        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_1,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_1),
                        }),
                    },
                    crate::ProgramDetails {
                        id: program_id_2,
                        state: crate::ProgramState::Active(crate::ProgramInfo {
                            static_pages,
                            persistent_pages: Default::default(),
                            code_hash: h256_code_hash(&code_2),
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

    let event = match System::events().last().map(|r| r.event.clone()) {
        Some(super::mock::RuntimeEvent::Gear(e)) => e,
        _ => unreachable!("Should be one Gear event"),
    };

    match event {
        Event::MessageQueued { id, .. } => id,
        Event::UserMessageSent { message, .. } => message.id(),
        _ => unreachable!("expect sending"),
    }
}

#[cfg(feature = "lazy-pages")]
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
            (import "env" "free" (func $free (param i32) (result i32)))
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
                    drop

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

                ;; store 1 to the begin of memory to identify that test goes right
                i32.const 0
                i32.const 1
                i32.store
            )
        )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = ProgramId::generate(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        assert_ok!(PalletGear::<Test>::upload_program(
            origin.clone(),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
        ));

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        GearDebug::do_snapshot();

        let gear_page0 = GearPage::from_offset(0x0);
        let mut page0_data = PageBuf::new_zeroed();
        page0_data[0] = 0x42;

        let gear_page7 = GearPage::from_offset(0x70000);
        let mut page7_data = PageBuf::new_zeroed();
        page7_data[0] = 0x42;

        let mut persistent_pages = BTreeMap::new();
        persistent_pages.insert(gear_page0, page0_data.clone());
        persistent_pages.insert(gear_page7, page7_data);

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages: 0.into(),
                        persistent_pages: persistent_pages.clone(),
                        code_hash: h256_code_hash(&code),
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

        GearDebug::do_snapshot();

        page0_data[0] = 0x1;
        persistent_pages.insert(gear_page0, page0_data);

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages: 0.into(),
                        persistent_pages: persistent_pages.clone(),
                        code_hash: h256_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );
    })
}

#[cfg(feature = "lazy-pages")]
#[test]
fn check_changed_pages_in_storage() {
    // This test checks that only pages, which has been write accessed,
    // will be stored in storage. Also it checks that data in storage is correct.
    let wat = r#"
        (module
            (import "env" "memory" (memory 8))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (import "env" "free" (func $free (param i32) (result i32)))
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
                i32.const 0x30     ;; write symbol "0" there
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
                drop

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
                drop
            )

            (func $handle
                (block
                    ;; check page 1 data
                    i32.const 0x10002
                    i64.load
                    i64.const 0x3038373635343332  ;; is symbols "23456780",
                                                  ;; "0" in the end because we change it in init
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
                ;; in 3th page only. But because we store by write access, then
                ;; both data will be for gear pages from 3th and 4th wasm page.
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
        let program_id = ProgramId::generate(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        // Code info. Must be in consensus with wasm code.
        let static_pages = 8.into();
        let page1_accessed_addr = 0x10000;
        let page3_accessed_addr = 0x3fffd;
        let page4_accessed_addr = 0x40000;
        let page8_accessed_addr = 0x87654;
        let page9_accessed_addr = 0x98765;

        assert_ok!(PalletGear::<Test>::upload_program(
            origin.clone(),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
        ));

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        GearDebug::do_snapshot();

        let mut persistent_pages = BTreeMap::new();

        let gear_page1 = GearPage::from_offset(page1_accessed_addr);
        let mut page1_data = PageBuf::new_zeroed();
        page1_data[..10].copy_from_slice(b"0123456780".as_slice());

        let gear_page8 = GearPage::from_offset(page8_accessed_addr);
        let mut page8_data = PageBuf::new_zeroed();
        page8_data[(page8_accessed_addr % GearPage::size()) as usize] = 0x42;

        let gear_page9 = GearPage::from_offset(page9_accessed_addr);
        let mut page9_data = PageBuf::new_zeroed();
        page9_data[(page9_accessed_addr % GearPage::size()) as usize] = 0x42;

        persistent_pages.insert(gear_page1, page1_data);
        persistent_pages.insert(gear_page8, page8_data);
        persistent_pages.insert(gear_page9, page9_data);

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages: persistent_pages.clone(),
                        code_hash: h256_code_hash(&code),
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

        GearDebug::do_snapshot();

        let gear_page3 = GearPage::from_offset(page3_accessed_addr);
        let mut page3_data = PageBuf::new_zeroed();
        page3_data[(page3_accessed_addr % GearPage::size()) as usize] = 0x42;

        let gear_page4 = GearPage::from_offset(page4_accessed_addr);

        persistent_pages.insert(gear_page3, page3_data);
        persistent_pages.insert(gear_page4, PageBuf::new_zeroed());

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages,
                        persistent_pages,
                        code_hash: h256_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );
    })
}

#[test]
fn check_gear_stack_end() {
    // This test checks that all pages, before stack end addr, must not be updated in storage.
    let wat = format!(
        r#"
        (module
            (import "env" "memory" (memory 4))
            (export "init" (func $init))
            (func $init
                ;; write to 0 wasm page (virtual stack)
                i32.const 0x0
                i32.const 0x42
                i32.store

                ;; write to 1 wasm page (virtual stack)
                i32.const 0x10000
                i32.const 0x42
                i32.store

                ;; write to 2 wasm page
                i32.const 0x20000
                i32.const 0x42
                i32.store

                ;; write to 3 wasm page
                i32.const 0x30000
                i32.const 0x42
                i32.store
            )
            ;; "stack" contains 0 and 1 wasm pages
            (global (;0;) (mut i32) (i32.const 0x20000))
            (export "{STACK_END_EXPORT_NAME}" (global 0))
        )
    "#
    );

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat.as_str());
        let program_id = ProgramId::generate(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        assert_ok!(PalletGear::<Test>::upload_program(
            origin,
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
        ));

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        GearDebug::do_snapshot();

        let mut persistent_pages = BTreeMap::new();

        let gear_page2 = WasmPage::from(2).to_page();
        let gear_page3 = WasmPage::from(3).to_page();
        let mut page_data = PageBuf::new_zeroed();
        page_data[0] = 0x42;

        persistent_pages.insert(gear_page2, page_data.clone());
        persistent_pages.insert(gear_page3, page_data);

        #[cfg(feature = "lazy-pages")]
        log::debug!("LAZY-PAGES IS ON");

        #[cfg(not(feature = "lazy-pages"))]
        log::debug!("LAZY-PAGES IS OFF");

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id,
                    state: crate::ProgramState::Active(crate::ProgramInfo {
                        static_pages: 4.into(),
                        persistent_pages,
                        code_hash: h256_code_hash(&code),
                    }),
                }],
            })
            .into(),
        );
    })
}
