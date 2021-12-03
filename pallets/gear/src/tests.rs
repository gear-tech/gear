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
use codec::Encode;
use common::{self, IntermediateMessage, Origin as _};
use frame_support::traits::{Currency, ExistenceRequirement};
use frame_support::{assert_noop, assert_ok};
use gear_core::program::{Program, ProgramId};
use hex_literal::hex;
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

#[test]
fn submit_program_works() {
    let wat = r#"
    (module
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);

        let messages: Option<Vec<IntermediateMessage>> = Gear::message_queue();
        assert!(messages.is_none());

        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");
        assert_eq!(messages.len(), 1);

        let (msg_origin, msg_code, program_id, message_id) = match &messages[0] {
            IntermediateMessage::InitProgram {
                origin,
                code,
                program_id,
                init_message_id,
                ..
            } => (*origin, code.to_vec(), *program_id, *init_message_id),
            _ => (
                Default::default(),
                Vec::new(),
                Default::default(),
                Default::default(),
            ),
        };
        assert_eq!(msg_origin, 1_u64.into_origin());
        assert_eq!(msg_code, code);
        System::assert_last_event(
            crate::Event::InitMessageEnqueued(crate::MessageInfo {
                message_id,
                program_id,
                origin: 1.into_origin(),
            })
            .into(),
        );
    })
}

#[test]
fn submit_program_expected_failure() {
    let wat = r#"
    (module
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);

        // Insufficient account balance to reserve gas
        assert_noop!(
            Pallet::<Test>::submit_program(
                Origin::signed(2),
                code.clone(),
                b"salt".to_vec(),
                Vec::new(),
                10_000_u64,
                10_u128
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Gas limit is too high
        assert_noop!(
            Pallet::<Test>::submit_program(
                Origin::signed(1),
                code.clone(),
                b"salt".to_vec(),
                Vec::new(),
                100_000_001_u64,
                0_u128
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}
#[test]
fn submit_program_fails_on_duplicate_id() {
    let wat = r#"(module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);

        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        // Finalize block to let queue processing run
        run_to_block(2, None);

        // By now this program id is already in the storage
        assert_noop!(
            Pallet::<Test>::submit_program(
                Origin::signed(1),
                code.clone(),
                b"salt".to_vec(),
                Vec::new(),
                10_000_u64,
                0_u128
            ),
            Error::<Test>::ProgramAlreadyExists
        );
    })
}

#[test]
fn send_message_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        // Make sure we have a program in the program storage
        let program_id = H256::from_low_u64_be(1001);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                    (export "handle" (func $handle))
                    (func $handle)
                )"#,
            ),
            Default::default(),
        )
        .unwrap();
        common::native::set_program(program);

        let messages: Option<Vec<IntermediateMessage>> = Gear::message_queue();
        assert!(messages.is_none());

        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            b"payload".to_vec(),
            10_000_u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");
        assert_eq!(messages.len(), 1);

        let mut id = b"payload".to_vec().encode();
        id.extend_from_slice(&0_u128.to_le_bytes());
        let id: H256 = sp_io::hashing::blake2_256(&id).into();

        let msg_id = match &messages[0] {
            IntermediateMessage::DispatchMessage { id, .. } => *id,
            _ => Default::default(),
        };
        assert_eq!(msg_id, id);

        // Sending message to a non-program address works as a simple value transfer
        // Gas limit is not transfered and returned back to sender (since operation is no-op).
        assert_eq!(Balances::free_balance(1), 99990000);
        assert_eq!(Balances::free_balance(2), 2);
        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            2.into_origin(),
            Vec::new(),
            10_000_u64,
            20_000_u128,
        ));
        // `value + gas_limit` have been deducted from the sender's balance
        assert_eq!(Balances::free_balance(1), 99_960_000);
        // However, only `value` has been transferred to the recepient yet
        assert_eq!(Balances::free_balance(2), 20_002);

        // The `gas_limit` part will be released to the sender in the next block
        run_to_block(2, Some(100_000));

        assert_eq!(Balances::free_balance(2), 20_002);

        // original sender gets back whatever gas_limit he used to send a message.
        assert_eq!(Balances::free_balance(1), 99_970_000);
    })
}

#[test]
fn send_message_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);

        // First, pretending the program panicked in init()
        ProgramsLimbo::<Test>::insert(program_id, 2.into_origin());
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(2).into(),
                program_id,
                b"payload".to_vec(),
                10_000_u64,
                0_u128
            ),
            Error::<Test>::ProgramIsNotInitialized
        );

        // This time the programs has made it to the storage
        ProgramsLimbo::<Test>::remove(program_id);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                )"#,
            ),
            Default::default(),
        )
        .expect("Program failed to instantiate");
        common::native::set_program(program);

        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(2).into(),
                program_id,
                b"payload".to_vec(),
                10_000_u64,
                0_u128
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Value tansfer is attempted if `value` field is greater than 0
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(2).into(),
                H256::from_low_u64_be(1002),
                b"payload".to_vec(),
                1_u64, // Must be greater than 0 to have changed the state during reserve()
                100_u128
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Gas limit too high
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(1).into(),
                program_id,
                b"payload".to_vec(),
                100_000_001_u64,
                0_u128
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn messages_processing_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
            i32.const 40000
            call $send
        )
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        MessageQueue::<Test>::put(vec![
            IntermediateMessage::InitProgram {
                origin: 1.into_origin(),
                code,
                program_id,
                init_message_id: H256::from_low_u64_be(1000001),
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(102),
                origin: 1.into_origin(),
                destination: program_id,
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
                reply: None,
            },
        ]);
        assert_eq!(
            Gear::message_queue()
                .expect("Failed to get messages from queue")
                .len(),
            2
        );

        crate::Pallet::<Test>::process_queue();
        System::assert_last_event(crate::Event::MessagesDequeued(2).into());

        // First message is sent to a non-existing program - and should get into log.
        // Second message still gets processed thereby adding 1 to the total processed messages counter.
        MessageQueue::<Test>::put(vec![
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(102),
                origin: 1.into_origin(),
                destination: 2.into_origin(),
                payload: Vec::new(),
                gas_limit: 10000,
                value: 100,
                reply: None,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(103),
                origin: 1.into_origin(),
                destination: program_id,
                payload: Vec::new(),
                gas_limit: 10000,
                value: 0,
                reply: None,
            },
        ]);
        crate::Pallet::<Test>::process_queue();
        // message with log destination should never get processed
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
    })
}

#[test]
fn spent_gas_to_reward_block_author_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
            i32.const 40000
            call $send
        )
        (func $init
            call $handle
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        let init_message_id = H256::from_low_u64_be(1000001);
        MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
            origin: 1.into_origin(),
            code,
            program_id,
            init_message_id,
            payload: "init".as_bytes().to_vec(),
            gas_limit: 10000,
            value: 0,
        }]);

        let block_author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);

        crate::Pallet::<Test>::process_queue();
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());

        // The block author should be paid the amount of Currency equal to
        // the `gas_charge` incurred while processing the `InitProgram` message
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_author_initial_balance.saturating_add(6_000)
        );
    })
}

#[test]
fn unused_gas_released_back_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
            i32.const 40000
            call $send
        )
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
            origin: 1.into_origin(),
            code,
            program_id,
            init_message_id: H256::from_low_u64_be(1000001),
            payload: "init".as_bytes().to_vec(),
            gas_limit: 5000_u64,
            value: 0_u128,
        }]);
        crate::Pallet::<Test>::process_queue();

        let external_origin_initial_balance = Balances::free_balance(1);
        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            Vec::new(),
            20_000_u64,
            0_u128,
        ));
        // send_message reserves balance on the sender's account
        assert_eq!(
            Balances::free_balance(1),
            external_origin_initial_balance.saturating_sub(20_000)
        );

        crate::Pallet::<Test>::process_queue();

        // Unused gas should be converted back to currency and released to the external origin
        assert_eq!(
            Balances::free_balance(1),
            external_origin_initial_balance.saturating_sub(10_000)
        );
    })
}

pub fn init_test_program(origin: H256, program_id: H256, wat: &str) {
    let code = parse_wat(wat);

    MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
        origin,
        code,
        program_id,
        init_message_id: H256::from_low_u64_be(1000001),
        payload: "init".as_bytes().to_vec(),
        gas_limit: 10_000_000_u64,
        value: 0_u128,
    }]);
    crate::Pallet::<Test>::process_queue();
}

#[test]
fn block_gas_limit_works() {
    // A module with $handle function being worth 6000 gas
    let wat1 = r#"
	(module
		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle
			i32.const 0
			i32.const 32
			i32.const 32
			i64.const 1000000000
			i32.const 1024
			i32.const 40000
			call $send
		)
		(func $init)
	)"#;

    // A module with $handle function being worth 94000 gas
    let wat2 = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $init)
        (func $doWork (param $size i32)
            (local $counter i32)
            i32.const 0
            set_local $counter
            loop $while
                get_local $counter
                i32.const 1
                i32.add
                set_local $counter
                get_local $counter
                get_local $size
                i32.lt_s
                if
                    br $while
                end
            end $while
        )
        (func $handle
            i32.const 10
            call $doWork
		)
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code1 = parse_wat(wat1);
        let code2 = parse_wat(wat2);
        let pid1 = H256::from_low_u64_be(1001);
        let pid2 = H256::from_low_u64_be(1002);

        MessageQueue::<Test>::put(vec![
            IntermediateMessage::InitProgram {
                origin: 1.into_origin(),
                code: code1,
                program_id: pid1,
                init_message_id: H256::from_low_u64_be(1000001),
                payload: Vec::new(),
                gas_limit: 10_000,
                value: 0,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(102),
                origin: 1.into_origin(),
                destination: pid1,
                payload: Vec::new(),
                gas_limit: 10_000,
                value: 0,
                reply: None,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(103),
                origin: 1.into_origin(),
                destination: pid1,
                payload: Vec::new(),
                gas_limit: 10_000,
                value: 100,
                reply: None,
            },
            IntermediateMessage::InitProgram {
                origin: 1.into_origin(),
                code: code2,
                program_id: pid2,
                init_message_id: H256::from_low_u64_be(1000002),
                payload: Vec::new(),
                gas_limit: 10_000,
                value: 0,
            },
        ]);

        // Run to block #2 where the queue processing takes place
        run_to_block(2, Some(100_000));
        System::assert_last_event(crate::Event::MessagesDequeued(4).into());

        // Run to the next block to reset the gas limit
        run_to_block(3, Some(100_000));

        assert!(MessageQueue::<Test>::get().is_none());

        // Add more messages to queue
        // Total `gas_limit` of three messages exceeds the block gas limit
        // Messages #1 abd #3 take 6000 gas
        // Message #2 takes 94000 gas
        MessageQueue::<Test>::put(vec![
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(104),
                origin: 1.into_origin(),
                destination: pid1,
                payload: Vec::new(),
                gas_limit: 10_000,
                value: 0,
                reply: None,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(105),
                origin: 1.into_origin(),
                destination: pid2,
                payload: Vec::new(),
                gas_limit: 95_000,
                value: 100,
                reply: None,
            },
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(106),
                origin: 1.into_origin(),
                destination: pid1,
                payload: Vec::new(),
                gas_limit: 20_000,
                value: 200,
                reply: None,
            },
        ]);

        run_to_block(4, Some(100_000));

        // Message #2 steps beyond the block gas allowance and is requeued
        // Message #1 is dequeued and processed, message #3 stays in the queue:
        //
        // | 1 |        | 3 |
        // | 2 |  ===>  | 2 |
        // | 3 |        |   |
        //
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
        assert_eq!(Gear::gas_allowance(), 90_000);

        // Run to the next block to reset the gas limit
        run_to_block(5, Some(100_000));

        // Message #3 get dequeued and processed
        // Message #2 gas limit still exceeds the remaining allowance:
        //
        // | 3 |        | 2 |
        // | 2 |  ===>  |   |
        //
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
        assert_eq!(Gear::gas_allowance(), 90_000);

        run_to_block(6, Some(100_000));

        // This time message #2 makes it into the block:
        //
        // | 2 |        |   |
        // |   |  ===>  |   |
        //
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
        assert_eq!(Gear::gas_allowance(), 11_000);
    });
}

#[test]
fn mailbox_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "gr_source" (func $gr_source (param i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (export "handle_reply" (func $handle_reply))
        (func $handle
            i32.const 16384
            call $gr_source
            i32.const 16384
            i32.const 0
            i32.const 32
            i64.const 1000000
            i32.const 1024
            i32.const 40000
            call $send
        )
        (func $handle_reply)
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);

        init_test_program(1.into_origin(), program_id, wat);

        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            Vec::new(),
            2_000_000_u64,
            0_u128,
        ));
        crate::Pallet::<Test>::process_queue();

        let mailbox_message = crate::Pallet::<Test>::remove_from_mailbox(
            1.into_origin(),
            // this is fixed (nonce based)
            hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a").into(),
        )
        .expect("There should be a message for user #1 in the mailbox");

        assert_eq!(
            mailbox_message.id,
            hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a").into(),
        );

        assert_eq!(mailbox_message.payload, vec![0u8; 32]);

        assert_eq!(mailbox_message.gas_limit, 1000000);
    })
}

#[test]
fn init_message_logging_works() {
    let wat1 = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init)
    )"#;

    let wat2 = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "init" (func $init))
        (func $doWork (param $size i32)
            (local $counter i32)
            i32.const 0
            set_local $counter
            loop $while
                get_local $counter
                i32.const 1
                i32.add
                set_local $counter
                get_local $counter
                get_local $size
                i32.lt_s
                if
                    br $while
                end
            end $while
        )
        (func $init
            i32.const 4
            call $doWork
		)
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat1);

        System::reset_events();

        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");

        let (program_id, message_id) = match &messages[0] {
            IntermediateMessage::InitProgram {
                program_id,
                init_message_id,
                ..
            } => (*program_id, *init_message_id),
            _ => Default::default(),
        };
        System::assert_last_event(
            crate::Event::InitMessageEnqueued(crate::MessageInfo {
                message_id,
                program_id,
                origin: 1.into_origin(),
            })
            .into(),
        );

        run_to_block(2, None);

        // Expecting the log to have an InitSuccess event
        System::assert_has_event(
            crate::Event::InitSuccess(crate::MessageInfo {
                message_id,
                program_id,
                origin: 1.into_origin(),
            })
            .into(),
        );

        let code = parse_wat(wat2);
        System::reset_events();
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");

        let (program_id, message_id) = match &messages[0] {
            IntermediateMessage::InitProgram {
                program_id,
                init_message_id,
                ..
            } => (*program_id, *init_message_id),
            _ => Default::default(),
        };
        System::assert_last_event(
            crate::Event::InitMessageEnqueued(crate::MessageInfo {
                message_id,
                program_id,
                origin: 1.into_origin(),
            })
            .into(),
        );

        run_to_block(3, None);

        // Expecting the log to have an InitFailure event (due to insufficient gas)
        System::assert_has_event(
            crate::Event::InitFailure(
                crate::MessageInfo {
                    message_id,
                    program_id,
                    origin: 1.into_origin(),
                },
                crate::Reason::Dispatch(hex!("48476173206c696d6974206578636565646564").into()),
            )
            .into(),
        );
    })
}

#[test]
fn program_lifecycle_works() {
    let wat1 = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init)
    )"#;

    let wat2 = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "init" (func $init))
        (func $doWork (param $size i32)
            (local $counter i32)
            i32.const 0
            set_local $counter
            loop $while
                get_local $counter
                i32.const 1
                i32.add
                set_local $counter
                get_local $counter
                get_local $size
                i32.lt_s
                if
                    br $while
                end
            end $while
        )
        (func $init
            i32.const 4
            call $doWork
		)
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat1);

        System::reset_events();

        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");
        let program_id = match &messages[0] {
            IntermediateMessage::InitProgram { program_id, .. } => *program_id,
            _ => Default::default(),
        };
        assert!(common::get_program(program_id).is_none());
        run_to_block(2, None);
        // Expect the program to be in PS by now
        assert!(common::get_program(program_id).is_some());

        // Submitting another program
        let code = parse_wat(wat2);
        System::reset_events();
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code.clone(),
            b"salt".to_vec(),
            Vec::new(),
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");
        let program_id = match &messages[0] {
            IntermediateMessage::InitProgram { program_id, .. } => *program_id,
            _ => Default::default(),
        };

        assert!(common::get_program(program_id).is_none());
        run_to_block(3, None);
        // Expect the program to have made it to the PS
        assert!(common::get_program(program_id).is_some());
        // while at the same time being stuck in "limbo"
        assert!(crate::Pallet::<Test>::is_uninitialized(program_id));
        assert_eq!(
            ProgramsLimbo::<Test>::get(program_id).unwrap(),
            1.into_origin()
        );
        // Program author is allowed to remove the program and reclaim funds
        // An attempt to remove a program on behalf of another account will fail
        assert_ok!(Pallet::<Test>::remove_stale_program(
            Origin::signed(2).into(), // Not the author
            program_id,
        ));
        // Program is still in the storage
        assert!(common::get_program(program_id).is_some());
        assert!(ProgramsLimbo::<Test>::get(program_id).is_some());

        assert_ok!(Pallet::<Test>::remove_stale_program(
            Origin::signed(1).into(),
            program_id,
        ));
        // This time the program has been removed
        assert!(common::get_program(program_id).is_none());
        assert!(ProgramsLimbo::<Test>::get(program_id).is_none());
    })
}

#[test]
fn events_logging_works() {
    let wat_ok = r#"
	(module
		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle
			i32.const 0
			i32.const 32
			i32.const 32
			i64.const 1000000
			i32.const 1024
            i32.const 40000
			call $send
		)
		(func $init)
	)"#;

    let wat_greedy_init = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "init" (func $init))
        (func $doWork (param $size i32)
            (local $counter i32)
            i32.const 0
            set_local $counter
            loop $while
                get_local $counter
                i32.const 1
                i32.add
                set_local $counter
                get_local $counter
                get_local $size
                i32.lt_s
                if
                    br $while
                end
            end $while
        )
        (func $init
            i32.const 4
            call $doWork
		)
	)"#;

    let wat_trap_in_handle = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle
			unreachable
		)
		(func $init)
	)"#;

    let wat_trap_in_init = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle)
		(func $init
            unreachable
        )
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code_ok = parse_wat(wat_ok);
        let code_greedy_init = parse_wat(wat_greedy_init);
        let code_trap_in_init = parse_wat(wat_trap_in_init);
        let code_trap_in_handle = parse_wat(wat_trap_in_handle);

        System::reset_events();

        // init ok
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code_ok.clone(),
            b"0001".to_vec(),
            vec![],
            10_000u64,
            0_u128
        ));
        // init out-of-gas
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code_greedy_init.clone(),
            b"0002".to_vec(),
            vec![],
            10_000u64,
            0_u128
        ));
        // init trapped
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code_trap_in_init.clone(),
            b"0003".to_vec(),
            vec![],
            10_000u64,
            0_u128
        ));
        // init ok
        assert_ok!(Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            code_trap_in_handle.clone(),
            b"0004".to_vec(),
            vec![],
            10_000u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");

        let mut init_msg = vec![];
        for message in messages {
            match message {
                IntermediateMessage::InitProgram {
                    program_id,
                    init_message_id,
                    ..
                } => {
                    init_msg.push((init_message_id, program_id));
                    System::assert_has_event(
                        crate::Event::InitMessageEnqueued(crate::MessageInfo {
                            message_id: init_message_id,
                            program_id,
                            origin: 1.into_origin(),
                        })
                        .into(),
                    );
                }
                _ => (),
            }
        }
        assert_eq!(init_msg.len(), 4);

        run_to_block(2, None);

        // Expecting programs 1 and 4 to have been inited successfully
        System::assert_has_event(
            crate::Event::InitSuccess(crate::MessageInfo {
                message_id: init_msg[0].0,
                program_id: init_msg[0].1,
                origin: 1.into_origin(),
            })
            .into(),
        );
        System::assert_has_event(
            crate::Event::InitSuccess(crate::MessageInfo {
                message_id: init_msg[3].0,
                program_id: init_msg[3].1,
                origin: 1.into_origin(),
            })
            .into(),
        );

        // Expecting programs 2 and 3 to have failed to init
        System::assert_has_event(
            crate::Event::InitFailure(
                crate::MessageInfo {
                    message_id: init_msg[1].0,
                    program_id: init_msg[1].1,
                    origin: 1.into_origin(),
                },
                crate::Reason::Dispatch(hex!("48476173206c696d6974206578636565646564").into()),
            )
            .into(),
        );
        System::assert_has_event(
            crate::Event::InitFailure(
                crate::MessageInfo {
                    message_id: init_msg[2].0,
                    program_id: init_msg[2].1,
                    origin: 1.into_origin(),
                },
                crate::Reason::Dispatch(vec![]),
            )
            .into(),
        );

        System::reset_events();

        // Sending messages to failed-to-init programs shouldn't be allowed
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(1).into(),
                init_msg[1].1,
                vec![],
                10_000_u64,
                0_u128
            ),
            Error::<Test>::ProgramIsNotInitialized
        );
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(1).into(),
                init_msg[2].1,
                vec![],
                10_000_u64,
                0_u128
            ),
            Error::<Test>::ProgramIsNotInitialized
        );

        // Messages to fully-initialized programs are accepted
        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            init_msg[0].1,
            vec![],
            10_000_000_u64,
            0_u128
        ));
        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            init_msg[3].1,
            vec![],
            10_000_u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");

        let mut dispatch_msg = vec![];
        for message in messages {
            match message {
                IntermediateMessage::DispatchMessage {
                    id,
                    destination,
                    origin,
                    ..
                } => {
                    dispatch_msg.push(id);
                    System::assert_has_event(
                        crate::Event::DispatchMessageEnqueued(crate::MessageInfo {
                            message_id: id,
                            program_id: destination,
                            origin,
                        })
                        .into(),
                    );
                }
                _ => (),
            }
        }
        assert_eq!(dispatch_msg.len(), 2);

        run_to_block(3, None);

        // First program completed successfully
        System::assert_has_event(
            crate::Event::MessageDispatched(DispatchOutcome {
                message_id: dispatch_msg[0],
                outcome: ExecutionResult::Success,
            })
            .into(),
        );
        // Fourth program failed to handle message
        System::assert_has_event(
            crate::Event::MessageDispatched(DispatchOutcome {
                message_id: dispatch_msg[1],
                outcome: ExecutionResult::Failure(vec![]),
            })
            .into(),
        );
    })
}

#[test]
fn send_reply_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        // Make sure we have a program in the program storage
        let program_id = H256::from_low_u64_be(1001);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                    (export "handle" (func $handle))
                    (func $handle)
                )"#,
            ),
            Default::default(),
        )
        .unwrap();
        common::native::set_program(program);

        let original_message_id = H256::from_low_u64_be(2002);
        Gear::insert_to_mailbox(
            1.into_origin(),
            common::Message {
                id: original_message_id.clone(),
                source: program_id.clone(),
                dest: 1.into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 0_u128,
                reply: None,
            },
        );

        assert_ok!(Pallet::<Test>::send_reply(
            Origin::signed(1).into(),
            original_message_id,
            b"payload".to_vec(),
            10_000_000_u64,
            0_u128
        ));

        let messages: Vec<IntermediateMessage> =
            Gear::message_queue().expect("There should be a message in the queue");
        assert_eq!(messages.len(), 1);

        let mut id = b"payload".to_vec().encode();
        id.extend_from_slice(&0_u128.to_le_bytes());
        let id: H256 = sp_io::hashing::blake2_256(&id).into();

        let (msg_id, orig_id) = match &messages[0] {
            IntermediateMessage::DispatchMessage { id, reply, .. } => (*id, reply.unwrap()),
            _ => Default::default(),
        };
        assert_eq!(msg_id, id);
        assert_eq!(orig_id, original_message_id);
    })
}

#[test]
fn send_reply_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                )"#,
            ),
            Default::default(),
        )
        .expect("Program failed to instantiate");
        common::native::set_program(program);

        let original_message_id = H256::from_low_u64_be(2002);

        // Expecting error as long as the user doesn't have messages in mailbox
        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(2).into(),
                original_message_id,
                b"payload".to_vec(),
                10_000_u64,
                0_u128
            ),
            Error::<Test>::NoMessageInMailbox
        );

        Gear::insert_to_mailbox(
            2.into_origin(),
            common::Message {
                id: original_message_id,
                source: program_id.clone(),
                dest: 2.into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 0_u128,
                reply: None,
            },
        );

        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(2).into(),
                original_message_id,
                b"payload".to_vec(),
                10_000_003_u64,
                0_u128
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Value tansfer is attempted if `value` field is greater than 0
        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(2).into(),
                original_message_id,
                b"payload".to_vec(),
                10_000_001_u64, // Must be greater than incoming gas_limit to have changed the state during reserve()
                100_u128,
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Gas limit too high
        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(1).into(),
                original_message_id,
                b"payload".to_vec(),
                100_000_001_u64,
                0_u128
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn send_reply_value_offset_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                )"#,
            ),
            Default::default(),
        )
        .expect("Program failed to instantiate");
        common::native::set_program(program);

        let original_message_id = H256::from_low_u64_be(2002);

        Gear::insert_to_mailbox(
            1.into_origin(),
            common::Message {
                id: original_message_id,
                source: program_id.clone(),
                dest: 1.into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 1_000_u128,
                reply: None,
            },
        );

        // Program doesn't have enough balance - error expected
        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(1).into(),
                original_message_id,
                b"payload".to_vec(),
                10_000_000_u64,
                0_u128
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        assert_ok!(
            <<Test as crate::Config>::Currency as Currency<_>>::transfer(
                &1,
                &<<Test as frame_system::Config>::AccountId as common::Origin>::from_origin(
                    program_id
                ),
                20_000_000,
                ExistenceRequirement::AllowDeath,
            )
        );
        assert_eq!(Balances::free_balance(1), 80_000_000);
        assert_eq!(Balances::reserved_balance(1), 0);

        assert_ok!(Pallet::<Test>::send_reply(
            Origin::signed(1).into(),
            original_message_id,
            b"payload".to_vec(),
            1_000_000_u64,
            100_u128,
        ));
        assert_eq!(Balances::free_balance(1), 89_000_900);
        assert_eq!(Balances::reserved_balance(1), 0);

        Gear::remove_from_mailbox(1.into_origin(), original_message_id);
        Gear::insert_to_mailbox(
            1.into_origin(),
            common::Message {
                id: original_message_id,
                source: program_id.clone(),
                dest: 1.into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 1_000_u128,
                reply: None,
            },
        );
        assert_ok!(Pallet::<Test>::send_reply(
            Origin::signed(1).into(),
            original_message_id,
            b"payload".to_vec(),
            20_000_000_u64,
            2_000_u128,
        ));
        assert_eq!(Balances::free_balance(1), 78_999_900);
        assert_eq!(Balances::reserved_balance(1), 10_000_000);
    })
}

#[test]
fn claim_value_from_mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);
        let program = Program::new(
            ProgramId::from_slice(&program_id[..]),
            parse_wat(
                r#"(module
                    (import "env" "memory" (memory 1))
                )"#,
            ),
            Default::default(),
        )
        .expect("Program failed to instantiate");
        common::native::set_program(program);

        let original_message_id = H256::from_low_u64_be(2002);
        common::value_tree::ValueView::get_or_create(
            GAS_VALUE_PREFIX,
            1.into_origin(),
            original_message_id.clone(),
            10_000_000,
        );

        Gear::insert_to_mailbox(
            1.into_origin(),
            common::Message {
                id: original_message_id,
                source: program_id.clone(),
                dest: 1.into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 1_000_u128,
                reply: None,
            },
        );

        // Program doesn't have enough balance - error expected
        assert_noop!(
            Pallet::<Test>::send_reply(
                Origin::signed(1).into(),
                original_message_id,
                b"payload".to_vec(),
                10_000_000_u64,
                0_u128
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        assert_ok!(
            <<Test as crate::Config>::Currency as Currency<_>>::transfer(
                &1,
                &<<Test as frame_system::Config>::AccountId as common::Origin>::from_origin(
                    program_id
                ),
                20_000_000,
                ExistenceRequirement::AllowDeath,
            )
        );
        assert_eq!(Balances::free_balance(1), 80_000_000);
        assert_eq!(Balances::reserved_balance(1), 0);

        assert_ok!(Pallet::<Test>::claim_value_from_mailbox(
            Origin::signed(1).into(),
            original_message_id,
        ));
        assert_eq!(Balances::free_balance(1), 80_001_000);
        assert_eq!(Balances::reserved_balance(1), 0);

        System::assert_last_event(
            crate::Event::ClaimedValueFromMailbox(original_message_id).into(),
        );
    })
}

pub fn generate_program_id(code: &[u8], salt: &[u8]) -> H256 {
    let mut data = Vec::new();
    code.encode_to(&mut data);
    salt.encode_to(&mut data);

    sp_io::hashing::blake2_256(&data[..]).into()
}

#[test]
fn distributor_initialize() {
    use tests_distributor::WASM_BINARY_BLOATY;

    new_test_ext().execute_with(|| {
        let initial_balance = Balances::free_balance(1) + Balances::free_balance(255);

        Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
            vec![],
            vec![],
            10_000_000_u64,
            0_u128,
        )
        .expect("Submit program failed");

        run_to_block(3, None);

        let final_balance = Balances::free_balance(1) + Balances::free_balance(255);
        assert_eq!(initial_balance, final_balance);
    });
}

#[test]
fn distributor_distribute() {
    use tests_distributor::{Request, WASM_BINARY_BLOATY};

    new_test_ext().execute_with(|| {
        let balance_initial = Balances::free_balance(1) + Balances::free_balance(255);

        let program_id =
            generate_program_id(WASM_BINARY_BLOATY.expect("Wasm binary missing!"), &[]);

        Pallet::<Test>::submit_program(
            Origin::signed(1).into(),
            WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
            vec![],
            vec![],
            10_000_000_u64,
            0_u128,
        )
        .expect("Submit program failed");

        Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            Request::Receive(10).encode(),
            20_000_000_u64,
            0_u128,
        )
        .expect("Send message failed");

        run_to_block(3, None);

        let final_balance = Balances::free_balance(1) + Balances::free_balance(255);

        assert_eq!(balance_initial, final_balance);
    });
}
