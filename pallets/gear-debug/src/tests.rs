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

use super::*;
use crate::mock::*;
use common::{self, Message, Origin as _};
use frame_support::assert_ok;
use runner::BlockInfo;
use sp_core::H256;

type Ext = gear_backend_sandbox::SandboxEnvironment<runner::Ext>;

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

        // Enable debug-mode
        DebugMode::<Test>::put(true);

        // Submit programs
        assert_ok!(runner::init_program::<Ext>(
            1.into_origin(),
            101.into_origin(),
            code_1.to_vec(),
            201.into_origin(),
            vec![],
            1_000_000_u64,
            0_u128,
            BlockInfo {
                height: 1_u32,
                timestamp: 1_000_000_000_u64,
            },
        ));

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                message_queue: vec![],
                programs: vec![crate::ProgramDetails {
                    id: 101.into_origin(),
                    static_pages: 16,
                    persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                    code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                    nonce: 0u64,
                }],
            })
            .into(),
        );

        assert_ok!(runner::init_program::<Ext>(
            1.into_origin(),
            102.into_origin(),
            code_2.to_vec(),
            202.into_origin(),
            vec![],
            1_000_000_u64,
            0_u128,
            BlockInfo {
                height: 1_u32,
                timestamp: 1_000_000_100_u64,
            },
        ));

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                message_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: 101.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: 102.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );

        // Enqueue messages
        common::queue_message(Message {
            id: 203.into_origin(),
            source: 1.into_origin(),
            payload: vec![],
            gas_limit: 1_000_000_u64,
            dest: 101.into_origin(), // code_1
            value: 0_u128,
            reply: None,
        });
        common::queue_message(Message {
            id: 204.into_origin(),
            source: 1.into_origin(),
            payload: vec![],
            gas_limit: 1_000_000_u64,
            dest: 102.into_origin(), // code_2
            value: 0_u128,
            reply: None,
        });

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                message_queue: vec![
                    Message {
                        id: 203.into_origin(),
                        source: 1.into_origin(),
                        payload: vec![],
                        gas_limit: 1_000_000_u64,
                        dest: 101.into_origin(), // code_1
                        value: 0_u128,
                        reply: None,
                    },
                    Message {
                        id: 204.into_origin(),
                        source: 1.into_origin(),
                        payload: vec![],
                        gas_limit: 1_000_000_u64,
                        dest: 102.into_origin(), // code_2
                        value: 0_u128,
                        reply: None,
                    },
                ],
                programs: vec![
                    crate::ProgramDetails {
                        id: 101.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: 102.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );

        // Process messages
        assert_ok!(runner::process::<Ext>(
            common::dequeue_message().expect("the queue should have the message; qed"),
            BlockInfo {
                height: 2_u32,
                timestamp: 1_000_001_000_u64,
            }
        ));

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                message_queue: vec![Message {
                    id: 204.into_origin(),
                    source: 1.into_origin(),
                    payload: vec![],
                    gas_limit: 1_000_000_u64,
                    dest: 102.into_origin(),
                    value: 0_u128,
                    reply: None,
                }],
                programs: vec![
                    crate::ProgramDetails {
                        id: 101.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                    crate::ProgramDetails {
                        id: 102.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );

        assert_ok!(runner::process::<Ext>(
            common::dequeue_message().expect("the queue should have the message; qed"),
            BlockInfo {
                height: 2_u32,
                timestamp: 1_000_001_200_u64,
            }
        ));

        Pallet::<Test>::do_snapshot();

        System::assert_last_event(
            crate::Event::DebugDataSnapshot(DebugData {
                message_queue: vec![],
                programs: vec![
                    crate::ProgramDetails {
                        id: 101.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..16).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_1)),
                        nonce: 0u64,
                    },
                    // handle() has allocated another 4 memory pages, hence 20 overall, not 16
                    crate::ProgramDetails {
                        id: 102.into_origin(),
                        static_pages: 16,
                        persistent_pages: (0..20).map(|i| (i, vec![0; 65536])).collect(),
                        code_hash: H256::from(sp_io::hashing::blake2_256(&code_2)),
                        nonce: 0u64,
                    },
                ],
            })
            .into(),
        );
    })
}
