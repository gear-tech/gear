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
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use gear_core::program::{Program, ProgramId};
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

        let (msg_origin, msg_code, id) = match &messages[0] {
            IntermediateMessage::InitProgram {
                origin,
                code,
                program_id,
                ..
            } => (*origin, code.to_vec(), *program_id),
            _ => (Default::default(), Vec::new(), Default::default()),
        };
        assert_eq!(msg_origin, 1_u64.into_origin());
        assert_eq!(msg_code, code);
        System::assert_last_event(crate::Event::NewProgram(id).into());
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
        assert_eq!(Balances::free_balance(1), 99990000);
        assert_eq!(Balances::free_balance(2), 1);
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
        assert_eq!(Balances::free_balance(2), 20_001);
        // The `gas_limit` part will be released to the recepient in the next block
        run_to_block(2, Some(100_000));
        assert_eq!(Balances::free_balance(2), 30_001);
    })
}

#[test]
fn send_message_expected_failure() {
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

        // Sending message to a non-program address triggers balance tansfer
        assert_noop!(
            Pallet::<Test>::send_message(
                Origin::signed(2).into(),
                H256::from_low_u64_be(1002),
                b"payload".to_vec(),
                0_u64,
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
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
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

        let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();

        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");
        System::assert_last_event(crate::Event::MessagesDequeued(2).into());

        // `InitProgram` doesn't increase the counter, but the reply message does; hence 1.
        assert_eq!(Gear::messages_processed(), 1);

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
        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");
        System::assert_last_event(crate::Event::MessagesDequeued(2).into());
        assert_eq!(Gear::messages_processed(), 3); // Counter not reset, 1 added
    })
}

#[test]
fn dequeue_limit_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
            call $send
        )
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        // Set dequeue limit
        DequeueLimit::<Test>::put(1);

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
            IntermediateMessage::DispatchMessage {
                id: H256::from_low_u64_be(103),
                origin: 1.into_origin(),
                destination: program_id,
                payload: Vec::new(),
                gas_limit: 10000,
                value: 100,
                reply: None,
            },
        ]);
        assert_eq!(
            Gear::message_queue()
                .expect("Failed to get messages from queue")
                .len(),
            3
        );

        let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();
        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");

        // Expect only one message to have been processed
        assert_eq!(Gear::messages_processed(), 1);
        System::assert_last_event(crate::Event::MessagesDequeued(2).into());

        // Put another message in queue
        MessageQueue::<Test>::put(vec![IntermediateMessage::DispatchMessage {
            id: H256::from_low_u64_be(104),
            origin: 1.into_origin(),
            destination: program_id,
            payload: Vec::new(),
            gas_limit: 10000,
            value: 200,
            reply: None,
        }]);
        assert_eq!(
            Gear::message_queue()
                .expect("Failed to get messages from queue")
                .len(),
            1
        );
        crate::Pallet::<Test>::process_queue(none_origin).expect("Failed to process queue");

        // This time we are already above the dequeue limit, hence no messages end up being processed
        assert_eq!(Gear::messages_processed(), 1);
    })
}

#[test]
fn spent_gas_to_reward_block_author_works() {
    let wat = r#"
    (module
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
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

        MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
            origin: 1.into_origin(),
            code,
            program_id,
            init_message_id: H256::from_low_u64_be(1000001),
            payload: "init".as_bytes().to_vec(),
            gas_limit: 10000,
            value: 0,
        }]);

        let block_author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);
        let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();

        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");
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
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle
            i32.const 0
            i32.const 32
            i32.const 32
            i64.const 1000000000
            i32.const 1024
            call $send
        )
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = parse_wat(wat);
        let program_id = H256::from_low_u64_be(1001);

        let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();

        MessageQueue::<Test>::put(vec![IntermediateMessage::InitProgram {
            origin: 1.into_origin(),
            code,
            program_id,
            init_message_id: H256::from_low_u64_be(1000001),
            payload: "init".as_bytes().to_vec(),
            gas_limit: 0_u64,
            value: 0_u128,
        }]);
        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");

        let external_origin_initial_balance = Balances::free_balance(1);
        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            Vec::new(),
            10_000_u64,
            0_u128,
        ));
        // send_message reserves balance on the sender's account
        assert_eq!(
            Balances::free_balance(1),
            external_origin_initial_balance.saturating_sub(10_000)
        );

        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");

        // Unused gas should be converted back to currency and released to the external origin
        assert_eq!(
            Balances::free_balance(1),
            external_origin_initial_balance.saturating_sub(6_000)
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
    crate::Pallet::<Test>::process_queue(RawOrigin::None.into()).expect("Failed to process queue");
}

#[test]
fn block_gas_limit_works() {
    // A module with $handle function being worth 6000 gas
    let wat1 = r#"
	(module
		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle
			i32.const 0
			i32.const 32
			i32.const 32
			i64.const 1000000000
			i32.const 1024
			call $send
		)
		(func $init)
	)"#;

    // A module with $handle function being worth 94000 gas
    let wat2 = r#"
	(module
		(import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
        (import "env" "gr_charge" (func $charge (param i64)))
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
        assert_eq!(Gear::gas_allowance(), 91_000);

        // Run to the next block to reset the gas limit
        run_to_block(5, Some(100_000));

        // Message #3 get dequeued and processed
        // Message #2 gas limit still exceeds the remaining allowance:
        //
        // | 3 |        | 2 |
        // | 2 |  ===>  |   |
        //
        System::assert_last_event(crate::Event::MessagesDequeued(1).into());
        assert_eq!(Gear::gas_allowance(), 91_000);

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
        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32)))
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
            call $send
        )
        (func $handle_reply)
        (func $init)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = H256::from_low_u64_be(1001);

        let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();

        init_test_program(1.into_origin(), program_id, wat);

        assert_ok!(Pallet::<Test>::send_message(
            Origin::signed(1).into(),
            program_id,
            Vec::new(),
            2_000_000_u64,
            0_u128,
        ));
        crate::Pallet::<Test>::process_queue(none_origin.clone()).expect("Failed to process queue");

        let mailbox_message = crate::remove_from_mailbox::<Test>(
            1.into_origin(),
            // this is fixed (nonce based)
            hex_literal::hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a")
                .into(),
        )
        .expect("There should be a message for user #1 in the mailbox");

        assert_eq!(
            mailbox_message.id,
            hex_literal::hex!("211a310ae0d68d7a4523ccecc7e5c0fd435496008c56ba8c86c5bba45d466e3a")
                .into(),
        );

        assert_eq!(mailbox_message.payload, vec![0u8; 32]);

        assert_eq!(mailbox_message.gas_limit, 1000000);
    })
}
