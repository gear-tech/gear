// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use common::{ActiveProgram, Origin, ProgramState, QueuedDispatch, QueuedMessage};
use frame_support::{assert_noop, assert_ok};
use gear_core::program::{CodeHash, InstrumentedCode};
use hex_literal::hex;
use sp_std::collections::btree_map::BTreeMap;

use super::*;
use crate::mock::*;

use utils::CreateProgramResult;

#[test]
fn pause_program_works() {
    new_test_ext().execute_with(|| {
        let code = hex!("0061736d01000000020f0103656e76066d656d6f7279020001").to_vec();
        let code_hash: H256 = CodeHash::generate(&code).into_origin();
        common::set_code(code_hash, &InstrumentedCode::new(code.clone(), 1, 1));

        let static_pages: u32 = 16;
        let memory_pages = {
            let mut pages = BTreeMap::new();
            pages.insert(static_pages, vec![static_pages as u8]);
            pages.insert(static_pages + 2, vec![static_pages as u8 + 2]);
            for i in 0..static_pages {
                pages.insert(i, vec![i as u8]);
            }

            pages
        };

        let program_id = H256::from_low_u64_be(1);

        common::set_program(
            program_id,
            ActiveProgram {
                static_pages,
                persistent_pages: memory_pages.clone().into_keys().collect(),
                code_hash,
                nonce: 0,
                state: ProgramState::Initialized,
            },
            memory_pages.clone(),
        );

        let msg_id_1 = H256::from_low_u64_be(1);
        common::insert_waiting_message(
            program_id,
            msg_id_1,
            QueuedDispatch::new_handle(QueuedMessage {
                id: msg_id_1,
                source: H256::from_low_u64_be(3),
                dest: program_id,
                payload: Default::default(),
                value: 0,
                reply: None,
            }),
            0,
        );

        let msg_id_2 = H256::from_low_u64_be(2);
        common::insert_waiting_message(
            program_id,
            msg_id_2,
            QueuedDispatch::new_handle(QueuedMessage {
                id: msg_id_2,
                source: H256::from_low_u64_be(4),
                dest: program_id,
                payload: Default::default(),
                value: 0,
                reply: None,
            }),
            0,
        );

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        assert!(GearProgram::program_paused(program_id));

        assert!(common::get_code(code_hash).is_some());

        // although the memory pages should be removed
        assert_eq!(
            common::get_program_pages(program_id, memory_pages.into_keys().collect()),
            None
        );

        assert!(common::remove_waiting_message(program_id, msg_id_1).is_none());
        assert!(common::remove_waiting_message(program_id, msg_id_2).is_none());
    });
}

#[test]
fn pause_program_twice_fails() {
    new_test_ext().execute_with(|| {
        let code = hex!("0061736d01000000020f0103656e76066d656d6f7279020001").to_vec();
        let code_hash: H256 = CodeHash::generate(&code).into_origin();
        common::set_code(code_hash, &InstrumentedCode::new(code.clone(), 1, 1));

        let program_id = H256::from_low_u64_be(1);
        let static_pages = 256;
        common::set_program(
            program_id,
            ActiveProgram {
                static_pages,
                persistent_pages: Default::default(),
                code_hash,
                nonce: 0,
                state: ProgramState::Initialized,
            },
            Default::default(),
        );

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));
        assert_noop!(
            GearProgram::pause_program(program_id),
            PauseError::ProgramNotFound
        );
    });
}

#[test]
fn pause_terminated_program_fails() {
    new_test_ext().execute_with(|| {
        let code = hex!("0061736d01000000020f0103656e76066d656d6f7279020001").to_vec();
        let code_hash: H256 = CodeHash::generate(&code).into_origin();
        common::set_code(code_hash, &InstrumentedCode::new(code.clone(), 1, 1));

        let program_id = H256::from_low_u64_be(1);
        let static_pages = 256;
        common::set_program(
            program_id,
            ActiveProgram {
                static_pages,
                persistent_pages: Default::default(),
                code_hash,
                nonce: 0,
                state: ProgramState::Initialized,
            },
            Default::default(),
        );

        run_to_block(2, None);

        assert_ok!(common::set_program_terminated_status(program_id));

        assert_noop!(
            GearProgram::pause_program(program_id),
            PauseError::ProgramTerminated
        );
    });
}

#[test]
fn pause_uninitialized_program_works() {
    new_test_ext().execute_with(|| {
        let static_pages = 16;
        let CreateProgramResult {
            program_id,
            code_hash,
            init_msg,
            msg_1,
            msg_2,
            memory_pages,
        } = utils::create_uninitialized_program_messages(static_pages);

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        assert!(GearProgram::program_paused(program_id));
        assert!(common::get_program(program_id).is_none());

        assert!(common::get_code(code_hash).is_some());

        // although the memory pages should be removed
        assert_eq!(
            common::get_program_pages(program_id, memory_pages.into_keys().collect()),
            None
        );

        assert!(common::remove_waiting_message(program_id, msg_1.message.id).is_none());
        assert!(common::remove_waiting_message(program_id, msg_2.message.id).is_none());
        assert!(common::remove_waiting_message(program_id, init_msg.message.id).is_none());

        assert!(common::waiting_init_take_messages(program_id).is_empty());
    });
}

#[test]
fn resume_uninitialized_program_works() {
    new_test_ext().execute_with(|| {
        let static_pages = 16;
        let CreateProgramResult {
            program_id,
            init_msg,
            msg_1,
            msg_2,
            memory_pages,
            ..
        } = utils::create_uninitialized_program_messages(static_pages);

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        let wait_list = IntoIterator::into_iter([&init_msg, &msg_1, &msg_2])
            .map(|d| (d.message.id, d.clone()))
            .collect::<BTreeMap<_, _>>();

        let block_number = 100;
        assert_ok!(GearProgram::resume_program_impl(
            program_id,
            memory_pages.clone(),
            wait_list,
            block_number
        ));
        assert!(!GearProgram::program_paused(program_id));

        let new_memory_pages =
            common::get_program_pages(program_id, memory_pages.clone().into_keys().collect())
                .unwrap();
        assert_eq!(memory_pages, new_memory_pages);

        let waiting_init = common::waiting_init_take_messages(program_id);
        assert_eq!(waiting_init.len(), 2);
        assert!(waiting_init.contains(&msg_1.message.id));
        assert!(waiting_init.contains(&msg_2.message.id));

        assert_eq!(
            block_number,
            common::remove_waiting_message(program_id, init_msg.message.id)
                .map(|(_, bn)| bn)
                .unwrap()
        );
        assert_eq!(
            block_number,
            common::remove_waiting_message(program_id, msg_1.message.id)
                .map(|(_, bn)| bn)
                .unwrap()
        );
        assert_eq!(
            block_number,
            common::remove_waiting_message(program_id, msg_2.message.id)
                .map(|(_, bn)| bn)
                .unwrap()
        );
    });
}

#[test]
fn resume_program_twice_fails() {
    new_test_ext().execute_with(|| {
        let static_pages = 16;
        let CreateProgramResult {
            program_id,
            memory_pages,
            init_msg,
            msg_1,
            msg_2,
            ..
        } = utils::create_uninitialized_program_messages(static_pages);

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        let wait_list = IntoIterator::into_iter([init_msg, msg_1, msg_2])
            .map(|d| (d.message.id, d))
            .collect::<BTreeMap<_, _>>();

        let block_number = 100;
        assert_ok!(GearProgram::resume_program_impl(
            program_id,
            memory_pages.clone(),
            wait_list.clone(),
            block_number
        ));
        assert_noop!(
            GearProgram::resume_program_impl(program_id, memory_pages, wait_list, block_number),
            Error::<Test>::PausedProgramNotFound
        );
    });
}

#[test]
fn resume_program_wrong_memory_fails() {
    new_test_ext().execute_with(|| {
        let static_pages = 16;
        let CreateProgramResult {
            program_id,
            mut memory_pages,
            init_msg,
            msg_1,
            msg_2,
            ..
        } = utils::create_uninitialized_program_messages(static_pages);

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        let block_number = 100;
        memory_pages.remove(&0);
        assert_noop!(
            GearProgram::resume_program_impl(
                program_id,
                memory_pages,
                IntoIterator::into_iter([init_msg, msg_1, msg_2])
                    .map(|d| (d.message.id, d))
                    .collect(),
                block_number
            ),
            Error::<Test>::WrongMemoryPages
        );
    });
}

#[test]
fn resume_program_wrong_list_fails() {
    new_test_ext().execute_with(|| {
        let static_pages = 16;
        let CreateProgramResult {
            program_id,
            memory_pages,
            init_msg,
            msg_1,
            mut msg_2,
            ..
        } = utils::create_uninitialized_program_messages(static_pages);

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        let block_number = 100;
        msg_2.message.payload = [0, 1, 2, 3, 4, 5].into();
        assert_noop!(
            GearProgram::resume_program_impl(
                program_id,
                memory_pages,
                IntoIterator::into_iter([init_msg, msg_1, msg_2])
                    .map(|d| (d.message.id, d))
                    .collect(),
                block_number
            ),
            Error::<Test>::WrongWaitList
        );
    });
}

mod utils {
    use super::*;

    pub struct CreateProgramResult {
        pub program_id: H256,
        pub code_hash: H256,
        pub init_msg: QueuedDispatch,
        pub msg_1: QueuedDispatch,
        pub msg_2: QueuedDispatch,
        pub memory_pages: BTreeMap<u32, Vec<u8>>,
    }

    pub fn create_uninitialized_program_messages(static_pages: u32) -> CreateProgramResult {
        let code = hex!("0061736d01000000020f0103656e76066d656d6f7279020001").to_vec();
        let code_hash: H256 = CodeHash::generate(&code).into_origin();
        common::set_code(code_hash, &InstrumentedCode::new(code.clone(), 1, 1));

        let memory_pages = {
            let mut pages = BTreeMap::<u32, Vec<u8>>::new();
            pages.insert(static_pages, vec![static_pages as u8]);
            pages.insert(static_pages + 2, vec![static_pages as u8 + 2]);
            for i in 0..static_pages {
                pages.insert(i, vec![i as u8]);
            }

            pages
        };

        let init_msg_id = H256::from_low_u64_be(3);
        let program_id = H256::from_low_u64_be(1);
        common::set_program(
            program_id,
            ActiveProgram {
                static_pages,
                persistent_pages: memory_pages.clone().into_keys().collect(),
                code_hash,
                nonce: 0,
                state: ProgramState::Uninitialized {
                    message_id: init_msg_id,
                },
            },
            memory_pages.clone(),
        );

        // init message
        let init_msg = QueuedDispatch::new_handle(QueuedMessage {
            id: init_msg_id,
            source: H256::from_low_u64_be(3),
            dest: program_id,
            payload: Default::default(),
            value: 0,
            reply: None,
        });
        common::insert_waiting_message(program_id, init_msg_id, init_msg.clone(), 0);

        let msg_id_1 = H256::from_low_u64_be(1);
        let msg_1 = QueuedDispatch::new_handle(QueuedMessage {
            id: msg_id_1,
            source: H256::from_low_u64_be(3),
            dest: program_id,
            payload: Default::default(),
            value: 0,
            reply: None,
        });
        common::insert_waiting_message(program_id, msg_id_1, msg_1.clone(), 0);
        common::waiting_init_append_message_id(program_id, msg_id_1);

        let msg_id_2 = H256::from_low_u64_be(2);
        let msg_2 = QueuedDispatch::new_handle(QueuedMessage {
            id: msg_id_2,
            source: H256::from_low_u64_be(4),
            dest: program_id,
            payload: Default::default(),
            value: 0,
            reply: None,
        });
        common::insert_waiting_message(program_id, msg_id_2, msg_2.clone(), 0);
        common::waiting_init_append_message_id(program_id, msg_id_2);

        CreateProgramResult {
            program_id,
            code_hash,
            init_msg,
            msg_1,
            msg_2,
            memory_pages,
        }
    }
}
