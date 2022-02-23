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

// TODO: deal with runner usage here

use super::*;
use crate::mock::*;
use codec::Encode;
use common::{self, Origin as _, QueuedDispatch, QueuedMessage};
use gear_core::message::DispatchKind;
use pallet_gear::DebugInfo;
use pallet_gear::Pallet as PalletGear;
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

pub fn program_id(code: &[u8]) -> H256 {
    let code_hash = sp_io::hashing::blake2_256(&code);
    let salt = b"salt".to_vec();
    let mut data = Vec::with_capacity(code_hash.len() + salt.len());
    code_hash.encode_to(&mut data);
    salt.encode_to(&mut data);

    sp_io::hashing::blake2_256(&data).into()
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

        let program_id_1 = program_id(&code_1);
        let program_id_2 = program_id(&code_2);

        PalletGear::<Test>::submit_program(
            Origin::signed(1).into(),
            code_1.clone(),
            b"salt".to_vec(),
            Vec::new(),
            1_000_000_u64,
            0_u128,
        )
        .expect("Failed to submit program");

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        run_to_block(2, None);

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: program_id_1,
                    static_pages: 16,
                    persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                    code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                    nonce: 0u64,
                }],
            })
            .into(),
        );

        PalletGear::<Test>::submit_program(
            Origin::signed(1).into(),
            code_2.clone(),
            b"salt".to_vec(),
            Vec::new(),
            1_000_000_u64,
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
                        static_pages: 16,
                        persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        static_pages: 16,
                        persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );

        PalletGear::<Test>::send_message(
            Origin::signed(1).into(),
            program_id_1,
            vec![],
            1_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_1 = common::peek_last_message_id(&[]);

        PalletGear::<Test>::send_message(
            Origin::signed(1).into(),
            program_id_2,
            vec![],
            1_000_000_u64,
            0_u128,
        )
        .expect("Failed to send message");

        let message_id_2 = common::peek_last_message_id(&[]);

        run_to_block(4, Some(0)); // no message will get processed

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                dispatch_queue: vec![
                    // message will have reverse order since the first one requeued to the end
                    QueuedDispatch {
                        kind: DispatchKind::Handle,
                        message: QueuedMessage {
                            id: message_id_2,
                            source: 1.into_origin(),
                            dest: program_id_2,
                            payload: vec![],
                            value: 0,
                            reply: None,
                        },
                        payload_store: None,
                    },
                    QueuedDispatch {
                        kind: DispatchKind::Handle,
                        message: QueuedMessage {
                            id: message_id_1,
                            source: 1.into_origin(),
                            dest: program_id_1,
                            payload: vec![],
                            value: 0,
                            reply: None,
                        },
                        payload_store: None,
                    },
                ],
                programs: vec![
                    crate::ProgramDetails {
                        id: program_id_2,
                        static_pages: 16,
                        persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        static_pages: 16,
                        persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
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
                        static_pages: 16,
                        persistent_pages: (0..20).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: program_id_1,
                        static_pages: 16,
                        persistent_pages: (0..16).map(|v| (v, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );
    })
}
