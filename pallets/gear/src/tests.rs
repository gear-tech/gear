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

use codec::Encode;
use frame_support::{assert_noop, assert_ok};
use frame_system::Pallet as SystemPallet;
use pallet_balances::{self, Pallet as BalancesPallet};

use common::{self, GasToFeeConverter, IntermediateMessage, Origin as _};
use tests_distributor::{Request, WASM_BINARY_BLOATY};

use super::{
    mock::{
        new_test_ext, run_to_block, Event as MockEvent, Origin, Test, BLOCK_AUTHOR,
        LOW_BALANCE_USER, USER_1, USER_2,
    },
    pallet, DispatchOutcome, Error, Event, ExecutionResult, GasAllowance, Mailbox, MessageInfo,
    Pallet as GearPallet, Reason,
};

use utils::*;

#[test]
fn submit_program_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Check MQ is empty
        assert!(GearPallet::<Test>::message_queue().is_none());

        assert_ok!(submit_program_default(USER_1, ProgramCodeKind::Default));

        let mq = GearPallet::<Test>::message_queue().expect("message was added to the queue");
        assert_eq!(mq.len(), 1);

        let (origin, code, program_id, message_id) = {
            let submit_msg = mq.into_iter().next().expect("mq length is 1");
            match submit_msg {
                IntermediateMessage::InitProgram {
                    origin,
                    code,
                    program_id,
                    init_message_id,
                    ..
                } => (origin, code, program_id, init_message_id),
                _ => unreachable!("only init program message is in the queue"),
            }
        };
        assert_eq!(origin, USER_1.into_origin());
        // submit_program_default submits ProgramCodeKind::Default
        assert_eq!(code, ProgramCodeKind::Default.to_bytes());

        SystemPallet::<Test>::assert_last_event(
            Event::InitMessageEnqueued(MessageInfo {
                message_id,
                program_id,
                origin,
            })
            .into(),
        );
    })
}

#[test]
fn submit_program_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let balance = BalancesPallet::<Test>::free_balance(USER_1);
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1).into(),
                ProgramCodeKind::Default.to_bytes(),
                DEFAULT_SALT.to_vec(),
                DEFAULT_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                balance + 1
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        assert_noop!(
            submit_program_default(LOW_BALANCE_USER, ProgramCodeKind::Default),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Gas limit is too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1).into(),
                ProgramCodeKind::Default.to_bytes(),
                DEFAULT_SALT.to_vec(),
                DEFAULT_PAYLOAD.to_vec(),
                block_gas_limit + 1,
                0
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn submit_program_fails_on_duplicate_id() {
    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(submit_program_default(USER_1, ProgramCodeKind::Default));
        // Finalize block to let queue processing run
        run_to_block(2, None);
        // By now this program id is already in the storage
        assert_noop!(
            submit_program_default(USER_1, ProgramCodeKind::Default),
            Error::<Test>::ProgramAlreadyExists
        );
    })
}

#[test]
fn send_message_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let user1_initial_balance = BalancesPallet::<Test>::free_balance(USER_1);
        let user2_initial_balance = BalancesPallet::<Test>::free_balance(USER_2);

        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        // After the submit program message will be sent, global nonce will be 1.
        let expected_msg_id = compute_user_message_id(DEFAULT_PAYLOAD, 1);

        assert_ok!(send_default_message(USER_1, program_id));

        let mq = GearPallet::<Test>::message_queue().expect("Two messages were sent");
        assert_eq!(mq.len(), 2);

        let actual_msg_id = {
            let sent_to_prog_msg = mq.into_iter().next_back().expect("mq is not empty");
            match sent_to_prog_msg {
                IntermediateMessage::DispatchMessage { id, .. } => id,
                _ => unreachable!("last message was a dispatch message"),
            }
        };

        assert_eq!(expected_msg_id, actual_msg_id);

        // Balances check
        // Gas spends on sending 2 default messages (submit program and send message to program)
        let user1_potential_msgs_spends = GasConverter::gas_to_fee(2 * DEFAULT_GAS_LIMIT);
        // User 1 has sent two messages
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );

        // Sending message to a non-program address works as a simple value transfer
        let mail_value = 20_000;
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            USER_2.into_origin(),
            DEFAULT_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            mail_value,
        ));
        let mail_spends = GasConverter::gas_to_fee(DEFAULT_GAS_LIMIT) + mail_value;

        // "Mail" deducts from the sender's balance `value + gas_limit`
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends - mail_spends
        );
        // However, only `value` has been transferred to the recipient yet
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_2),
            user2_initial_balance + mail_value
        );

        // The `gas_limit` part will be released to the sender in the next block
        let remaining_weight = 100_000;
        run_to_block(2, Some(remaining_weight));
        // Messages were sent by user 1 only
        let user1_actual_msgs_spends =
            GasConverter::gas_to_fee(remaining_weight - GasAllowance::<Test>::get());

        // Balance of user 2 is the same
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_2),
            user2_initial_balance + mail_value
        );
        // Corrected by the actual amount of spends for sending messages
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends - mail_spends
        );
    });
}

#[test]
fn send_message_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Submitting failing in init program and check message is failed to be sent to it
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        run_to_block(2, None);

        assert_noop!(
            send_default_message(LOW_BALANCE_USER, program_id),
            Error::<Test>::ProgramIsNotInitialized
        );

        // Submit valid program and test failing actions on it
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_noop!(
            send_default_message(LOW_BALANCE_USER, program_id),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Value transfer is attempted if `value` field is greater than 0
        assert_noop!(
            GearPallet::<Test>::send_message(
                Origin::signed(LOW_BALANCE_USER).into(),
                USER_1.into_origin(),
                DEFAULT_PAYLOAD.to_vec(),
                1, // gas limit must be greater than 0 to have changed the state during reserve()
                100
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Gas limit too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            GearPallet::<Test>::send_message(
                Origin::signed(USER_1).into(),
                program_id,
                DEFAULT_PAYLOAD.to_vec(),
                block_gas_limit + 1,
                0
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn messages_processing_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        assert_ok!(send_default_message(USER_1, program_id));

        run_to_block(2, None);

        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());

        assert_ok!(send_default_message(USER_1, USER_2.into_origin()));
        assert_ok!(send_default_message(USER_1, program_id));

        run_to_block(3, None);

        // "Mail" from user to user should not be processed as messages
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());
    });
}

#[test]
fn spent_gas_to_reward_block_author_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let block_author_initial_balance = BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);
        assert_ok!(submit_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(2, None);

        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());

        // The block author should be paid the amount of Currency equal to
        // the `gas_charge` incurred while processing the `InitProgram` message
        let gas_spent = GasConverter::gas_to_fee(
            <Test as pallet::Config>::BlockGasLimit::get() - GasAllowance::<Test>::get(),
        );
        assert_eq!(
            BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR),
            block_author_initial_balance + gas_spent
        );
    })
}

#[test]
fn unused_gas_released_back_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let user1_initial_balance = BalancesPallet::<Test>::free_balance(USER_1);
        let huge_send_message_gas_limit = 50_000;

        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            program_id,
            DEFAULT_PAYLOAD.to_vec(),
            huge_send_message_gas_limit,
            0
        ));
        // Spends for submit program with default gas limit and sending default message with a huge gas limit
        let user1_potential_msgs_spends =
            GasConverter::gas_to_fee(DEFAULT_GAS_LIMIT + huge_send_message_gas_limit);
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );

        run_to_block(2, None);
        let user1_actual_msgs_spends = GasConverter::gas_to_fee(
            <Test as pallet::Config>::BlockGasLimit::get() - GasAllowance::<Test>::get(),
        );
        assert!(user1_potential_msgs_spends > user1_actual_msgs_spends);
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends
        );
    })
}

#[test]
fn block_gas_limit_works() {
    // Same as `ProgramCodeKind::GreedyInit`, but greedy handle
    let wat = r#"
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
        let remaining_weight = 100_000;

        // Submit programs and get their ids
        let pid1 = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        let pid2 = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Custom(wat));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, Some(remaining_weight));
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());

        // Count gas needed to process programs with default payload
        let expected_gas_msg_to_pid1 =
            GearPallet::<Test>::get_gas_spent(pid1, DEFAULT_PAYLOAD.to_vec())
                .expect("internal error: get gas spent (pid1) failed");
        let expected_gas_msg_to_pid2 =
            GearPallet::<Test>::get_gas_spent(pid2, DEFAULT_PAYLOAD.to_vec())
                .expect("internal error: get gas spent (pid2) failed");

        // TrapInHandle code kind is used because processing default payload in its
        // context requires such an amount of gas, that the following assertion can be passed.
        assert!(expected_gas_msg_to_pid1 + expected_gas_msg_to_pid2 > remaining_weight);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            pid1,
            DEFAULT_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            100
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            pid1,
            DEFAULT_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            100
        ));

        run_to_block(3, Some(remaining_weight));
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());

        // Run to the next block to reset the gas limit
        run_to_block(4, Some(remaining_weight));

        assert!(GearPallet::<Test>::message_queue().is_none());

        // Add more messages to queue
        // Total `gas_limit` of three messages (2 to pid1 and 1 to pid2) exceeds the block gas limit
        assert!(remaining_weight < 2 * expected_gas_msg_to_pid1 + expected_gas_msg_to_pid2);
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            pid1,
            DEFAULT_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            200
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            pid2,
            DEFAULT_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid2,
            100
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            pid1,
            DEFAULT_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            200
        ));

        // Try to process 3 messages
        run_to_block(5, Some(remaining_weight));

        // Message #2 steps beyond the block gas allowance and is re-queued
        // Message #1 is dequeued and processed, message #3 stays in the queue:
        //
        // | 1 |        | 3 |
        // | 2 |  ===>  | 2 |
        // | 3 |        |   |
        //
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());
        assert_eq!(
            GasAllowance::<Test>::get(),
            remaining_weight - expected_gas_msg_to_pid1
        );

        // Try to process 2 messages
        run_to_block(6, Some(remaining_weight));

        // Message #3 get dequeued and processed
        // Message #2 gas limit still exceeds the remaining allowance:
        //
        // | 3 |        | 2 |
        // | 2 |  ===>  |   |
        //
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());
        assert_eq!(
            GasAllowance::<Test>::get(),
            remaining_weight - expected_gas_msg_to_pid1
        );

        run_to_block(7, Some(remaining_weight));

        // This time message #2 makes it into the block:
        //
        // | 2 |        |   |
        // |   |  ===>  |   |
        //
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());
        assert_eq!(
            GasAllowance::<Test>::get(),
            remaining_weight - expected_gas_msg_to_pid2
        );
    });
}

#[test]
fn mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_1);

        let mailbox_message = {
            let res = GearPallet::<Test>::remove_from_mailbox(USER_1.into_origin(), reply_to_id);
            assert!(res.is_some());
            res.expect("was asserted previously")
        };

        assert_eq!(mailbox_message.id, reply_to_id,);
        // Value was taken from the program code!
        assert_eq!(mailbox_message.gas_limit, 10_000_000);
        assert_eq!(mailbox_message.value, 1000);
    })
}

#[test]
fn init_message_logging_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let mut next_block = 2;
        let codes = [
            (ProgramCodeKind::Default, false, ""),
            // Will fail, because tests use default gas limit, which is very low for successful greedy init
            (ProgramCodeKind::GreedyInit, true, "Gas limit exceeded"),
        ];

        for (code_kind, is_failing, trap_explanation) in codes {
            SystemPallet::<Test>::reset_events();

            assert_ok!(submit_program_default(USER_1, code_kind));

            let (program_id, message_id, origin) = {
                let msg: IntermediateMessage = GearPallet::<Test>::message_queue()
                    .map(|v| v.into_iter().next())
                    .flatten()
                    .expect("mq has only submit program message");
                match msg {
                    IntermediateMessage::InitProgram {
                        program_id,
                        init_message_id,
                        origin,
                        ..
                    } => (program_id, init_message_id, origin),
                    _ => unreachable!("mq has only submit program message"),
                }
            };

            SystemPallet::<Test>::assert_last_event(
                Event::InitMessageEnqueued(MessageInfo {
                    message_id,
                    program_id,
                    origin,
                })
                .into(),
            );

            run_to_block(next_block, None);

            let msg_info = MessageInfo {
                message_id,
                program_id,
                origin,
            };

            SystemPallet::<Test>::assert_has_event(if is_failing {
                let trap_explanation = String::from(trap_explanation).encode();
                Event::InitFailure(msg_info, Reason::Dispatch(trap_explanation)).into()
            } else {
                Event::InitSuccess(msg_info).into()
            });

            next_block += 1;
        }
    })
}

#[test]
fn program_lifecycle_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Submitting first program and getting its id
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(common::get_program(program_id).is_none());
        run_to_block(2, None);
        // Expect the program to be in PS by now
        assert!(common::get_program(program_id).is_some());

        // Submitting second program, which fails on initialization, therefore goes to limbo.
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(common::get_program(program_id).is_none());
        run_to_block(3, None);
        // Expect the program to have made it to the PS
        assert!(common::get_program(program_id).is_some());
        // while at the same time being stuck in "limbo"
        assert!(GearPallet::<Test>::is_uninitialized(program_id));

        // Program author is allowed to remove the program and reclaim funds
        // An attempt to remove a program on behalf of another account will make no changes
        assert_ok!(GearPallet::<Test>::remove_stale_program(
            Origin::signed(LOW_BALANCE_USER).into(), // Not the author
            program_id,
        ));
        // Program is still in the storage
        assert!(common::get_program(program_id).is_some());
        // and is still in the limbo
        assert!(GearPallet::<Test>::is_uninitialized(program_id));

        assert_ok!(GearPallet::<Test>::remove_stale_program(
            Origin::signed(USER_1).into(),
            program_id,
        ));
        // This time the program has been removed
        assert!(common::get_program(program_id).is_none());
        assert!(crate::ProgramsLimbo::<Test>::get(program_id).is_none());
    })
}

#[test]
fn events_logging_works() {
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
        let mut nonce = 0;
        let mut next_block = 2;
        let tests = [
            // Code, init failure reason, handle succeed flag
            (ProgramCodeKind::Default, None, true),
            (
                ProgramCodeKind::GreedyInit,
                Some(String::from("Gas limit exceeded").encode()),
                false,
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_init),
                Some(Vec::new()),
                false,
            ),
            (ProgramCodeKind::Custom(wat_trap_in_handle), None, false),
        ];
        for (code_kind, init_failure_reason, handle_succeed) in tests {
            SystemPallet::<Test>::reset_events();

            let program_id = {
                let res = submit_program_default(USER_1, code_kind);
                assert_ok!(res);
                res.expect("submit result was asserted")
            };

            let init_msg_info = MessageInfo {
                program_id,
                message_id: compute_user_message_id(DEFAULT_PAYLOAD, nonce),
                origin: USER_1.into_origin(),
            };
            nonce += 1;

            SystemPallet::<Test>::assert_last_event(
                Event::InitMessageEnqueued(init_msg_info.clone()).into(),
            );

            run_to_block(next_block, None);
            next_block += 1;

            // Init failed program checks
            if let Some(init_failure_reason) = init_failure_reason {
                SystemPallet::<Test>::assert_has_event(
                    Event::InitFailure(init_msg_info, Reason::Dispatch(init_failure_reason)).into(),
                );
                // Sending messages to failed-to-init programs shouldn't be allowed
                assert_noop!(
                    send_default_message(USER_1, program_id),
                    Error::<Test>::ProgramIsNotInitialized
                );
                continue;
            }

            SystemPallet::<Test>::assert_has_event(Event::InitSuccess(init_msg_info).into());

            let dispatch_msg_info = MessageInfo {
                program_id,
                message_id: compute_user_message_id(DEFAULT_PAYLOAD, nonce),
                origin: USER_1.into_origin(),
            };
            // Messages to fully-initialized programs are accepted
            assert_ok!(send_default_message(USER_1, program_id));
            SystemPallet::<Test>::assert_last_event(
                Event::DispatchMessageEnqueued(dispatch_msg_info.clone()).into(),
            );

            run_to_block(next_block, None);

            SystemPallet::<Test>::assert_has_event(
                Event::MessageDispatched(DispatchOutcome {
                    message_id: dispatch_msg_info.message_id,
                    outcome: if handle_succeed {
                        ExecutionResult::Success
                    } else {
                        ExecutionResult::Failure(Vec::new())
                    },
                })
                .into(),
            );

            nonce += 1;
            next_block += 1;
        }
    })
}

#[test]
fn send_reply_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_1);

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1).into(),
            reply_to_id,
            DEFAULT_PAYLOAD.to_vec(),
            10_000_000,
            1000, // `prog_id` sent message with value of 1000 (see program code)
        ));

        // global nonce is 2 before sending reply message
        // `submit_program` and `send_message` messages were sent before in `setup_mailbox_test_state`
        let expected_reply_message_id = compute_user_message_id(DEFAULT_PAYLOAD, 2);
        let (actual_reply_message_id, orig_id) = {
            let intermediate_msg = GearPallet::<Test>::message_queue()
                .map(|v| v.into_iter().next())
                .flatten()
                .expect("reply message was previously sent");
            match intermediate_msg {
                IntermediateMessage::DispatchMessage { id, reply, .. } => {
                    (id, reply.expect("was a reply message"))
                }
                _ => unreachable!("only reply message was in mq"),
            }
        };

        assert_eq!(expected_reply_message_id, actual_reply_message_id);
        assert_eq!(orig_id, reply_to_id);
    })
}

#[test]
fn send_reply_insufficient_program_balance() {
    init_logger();
    new_test_ext().execute_with(|| {
        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_1);

        // Program doesn't have enough balance - error expected
        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(USER_1).into(),
                reply_to_id,
                DEFAULT_PAYLOAD.to_vec(),
                5_000_000,
                0
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );
    })
}

#[test]
fn send_reply_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Expecting error as long as the user doesn't have messages in mailbox
        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(LOW_BALANCE_USER).into(),
                5.into_origin(), // non existent `reply_to_id`
                DEFAULT_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                0
            ),
            Error::<Test>::NoMessageInMailbox
        );

        // Submitting program and sending it message to invoke a message, that will be added to LOW_BALANCE_USER's sandbox
        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // increase LOW_BALANCE_USER balance a bit to allow him send message
        let reply_gas_spent = GearPallet::<Test>::get_gas_spent(prog_id, DEFAULT_PAYLOAD.to_vec())
            .expect("program exists and not faulty");
        assert_ok!(BalancesPallet::<Test>::transfer(
            Origin::signed(USER_1).into(),
            LOW_BALANCE_USER,
            GasConverter::gas_to_fee(reply_gas_spent),
        ));

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(LOW_BALANCE_USER).into(),
            prog_id,
            DEFAULT_PAYLOAD.to_vec(),
            reply_gas_spent,
            0,
        ));

        // Newly created program, which sends message in handle, has received only one message by now => nonce is 0.
        let reply_to_id = compute_program_message_id(prog_id.as_bytes(), 0);

        run_to_block(2, None);

        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(LOW_BALANCE_USER).into(),
                reply_to_id,
                DEFAULT_PAYLOAD.to_vec(),
                10_000_000, // Too big gas limit value
                1000
            ),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Value transfer is attempted if `value` field is greater than 0
        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(LOW_BALANCE_USER).into(),
                reply_to_id,
                DEFAULT_PAYLOAD.to_vec(),
                1, // Must be greater than incoming gas_limit to have changed the state during reserve()
                1000,
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Gas limit too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(USER_1).into(),
                reply_to_id,
                DEFAULT_PAYLOAD.to_vec(),
                block_gas_limit + 1,
                1000
            ),
            Error::<Test>::GasLimitTooHigh
        );
    })
}

#[test]
fn send_reply_value_offset_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // These values are actually constants in WAT. Alternatively can be read from Mailbox.
        let locked_gas_limit = 10_000_000;
        let locked_value = 1000;

        let mut next_block = 2;
        let mut program_nonce = 0u64;

        let user_messages_data = [
            // gas limit, value
            (1_000_000, 100),
            (20_000_000, 2000),
        ];
        for (gas_limit_to_reply, value_to_reply) in user_messages_data {
            let reply_to_id =
                populate_mailbox_from_program(prog_id, USER_1, next_block, program_nonce);
            program_nonce += 1;
            next_block += 1;

            let user_balance = BalancesPallet::<Test>::free_balance(USER_1);

            let send_to_program_amount = GasConverter::gas_to_fee(locked_gas_limit) * 2;
            assert_ok!(
                <BalancesPallet::<Test> as frame_support::traits::Currency<_>>::transfer(
                    &USER_1,
                    &AccountId::from_origin(prog_id),
                    send_to_program_amount,
                    frame_support::traits::ExistenceRequirement::AllowDeath
                )
            );
            assert_eq!(
                BalancesPallet::<Test>::free_balance(USER_1),
                user_balance - send_to_program_amount
            );
            assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

            assert_ok!(GearPallet::<Test>::send_reply(
                Origin::signed(USER_1).into(),
                reply_to_id,
                DEFAULT_PAYLOAD.to_vec(),
                gas_limit_to_reply,
                value_to_reply,
            ));

            let user_expected_balance = user_balance
                - send_to_program_amount
                - value_to_reply
                - GasConverter::gas_to_fee(gas_limit_to_reply)
                + locked_value
                + GasConverter::gas_to_fee(locked_gas_limit);
            assert_eq!(
                BalancesPallet::<Test>::free_balance(USER_1),
                user_expected_balance
            );
            assert_eq!(
                BalancesPallet::<Test>::reserved_balance(USER_1),
                GasConverter::gas_to_fee(gas_limit_to_reply.saturating_sub(locked_gas_limit))
            );
        }
    })
}

#[test]
fn claim_value_from_mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // These values are actually constants in WAT. Alternatively can be read from Mailbox.
        let locked_gas_limit = 10_000_000;
        let locked_value = 1000;

        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        let reply_to_id = populate_mailbox_from_program(prog_id, USER_1, 2, 0);

        // TODO Must solve #539 to remove that clumsy creation
        common::value_tree::ValueView::get_or_create(
            common::GAS_VALUE_PREFIX,
            USER_1.into_origin(),
            reply_to_id,
            locked_gas_limit,
        );

        let user_balance = BalancesPallet::<Test>::free_balance(USER_1);

        let send_to_program_amount = GasConverter::gas_to_fee(locked_gas_limit) * 2;
        assert_ok!(
            <BalancesPallet::<Test> as frame_support::traits::Currency<_>>::transfer(
                &USER_1,
                &AccountId::from_origin(prog_id),
                send_to_program_amount,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );

        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user_balance - send_to_program_amount
        );

        assert_ok!(GearPallet::<Test>::claim_value_from_mailbox(
            Origin::signed(USER_1).into(),
            reply_to_id,
        ));

        // TODO #539 after claim some amount locked for gas in message must be returned
        let expected_balance = user_balance - send_to_program_amount + locked_value;
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            expected_balance
        );
        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

        SystemPallet::<Test>::assert_last_event(Event::ClaimedValueFromMailbox(reply_to_id).into());
    })
}

#[test]
fn distributor_initialize() {
    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1).into(),
            WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
            DEFAULT_SALT.to_vec(),
            DEFAULT_PAYLOAD.to_vec(),
            10_000_000,
            0,
        ));

        run_to_block(2, None);

        let final_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);
        assert_eq!(initial_balance, final_balance);
    });
}

#[test]
fn distributor_distribute() {
    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);

        let program_id = generate_program_id(
            WASM_BINARY_BLOATY.expect("Wasm binary missing!"),
            DEFAULT_SALT,
        );

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1).into(),
            WASM_BINARY_BLOATY.expect("Wasm binary missing!").to_vec(),
            DEFAULT_SALT.to_vec(),
            DEFAULT_PAYLOAD.to_vec(),
            10_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1).into(),
            program_id,
            Request::Receive(10).encode(),
            20_000_000,
            0,
        ));

        run_to_block(3, None);

        let final_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);

        assert_eq!(initial_balance, final_balance);
    });
}

// TODO #512 All `submit_code` tests should be changed to testing program creation from program logic.

#[test]
fn test_code_submission_pass() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = sp_io::hashing::blake2_256(&code).into();

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            code.clone()
        ));

        let saved_code = common::get_code(code_hash);
        assert_eq!(saved_code, Some(code));

        let expected_meta = Some(common::CodeMetadata::new(USER_1.into_origin(), 1));
        let actual_meta = common::get_code_metadata(code_hash);
        assert_eq!(expected_meta, actual_meta);

        SystemPallet::<Test>::assert_last_event(Event::CodeSaved(code_hash).into());
    })
}

#[test]
fn test_same_code_submission_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            code.clone()
        ),);
        // Trying to set the same code twice.
        assert_noop!(
            GearPallet::<Test>::submit_code(Origin::signed(USER_1), code.clone()),
            Error::<Test>::CodeAlreadyExists,
        );
        // Trying the same from another origin
        assert_noop!(
            GearPallet::<Test>::submit_code(Origin::signed(USER_2), code.clone()),
            Error::<Test>::CodeAlreadyExists,
        );
    })
}

#[test]
fn test_code_is_not_submitted_twice_after_program_submission() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = sp_io::hashing::blake2_256(&code).into();

        // First submit program, which will set code and metadata
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1).into(),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            DEFAULT_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        SystemPallet::<Test>::assert_has_event(Event::CodeSaved(code_hash).into());
        assert!(common::code_exists(code_hash));

        // Trying to set the same code twice.
        assert_noop!(
            GearPallet::<Test>::submit_code(Origin::signed(USER_2), code),
            Error::<Test>::CodeAlreadyExists,
        );
    })
}

#[test]
fn test_code_is_not_resetted_within_program_submission() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = sp_io::hashing::blake2_256(&code).into();

        // First submit code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            code.clone()
        ));
        let expected_code_saved_events = 1;
        let expected_meta = common::get_code_metadata(code_hash);
        assert!(expected_meta.is_some());

        // Submit program from another origin. Should not change meta or code.
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2).into(),
            code,
            DEFAULT_SALT.to_vec(),
            DEFAULT_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        let actual_meta = common::get_code_metadata(code_hash);
        let actual_code_saved_events = SystemPallet::<Test>::events()
            .iter()
            .filter(|e| matches!(e.event, MockEvent::Gear(Event::CodeSaved(_))))
            .count();

        assert_eq!(expected_meta, actual_meta);
        assert_eq!(expected_code_saved_events, actual_code_saved_events);
    })
}

mod utils {
    use codec::Encode;
    use frame_support::dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo};
    use sp_core::H256;

    use super::{assert_ok, pallet, run_to_block, GearPallet, Mailbox, Origin, Test};

    pub(super) const DEFAULT_GAS_LIMIT: u64 = 10_000;
    pub(super) const DEFAULT_SALT: &'static [u8; 4] = b"salt";
    pub(super) const DEFAULT_PAYLOAD: &'static [u8; 7] = b"payload";

    pub(super) type DispatchCustomResult<T> = Result<T, DispatchErrorWithPostInfo>;
    pub(super) type AccountId = <Test as frame_system::Config>::AccountId;
    pub(super) type GasConverter = <Test as pallet::Config>::GasConverter;
    type BlockNumber = <Test as frame_system::Config>::BlockNumber;

    pub(super) fn init_logger() {
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    // Creates a new program and puts message from program to `user` in mailbox
    // using extrinsic calls. Imitates real-world sequence of calls.
    //
    // *NOTE*:
    // 1) usually called inside first block
    // 2) runs to block 2 all the messages place to message queue/storage
    //
    // Returns id of the message in the mailbox
    pub(super) fn setup_mailbox_test_state(user: AccountId) -> H256 {
        let prog_id = {
            let res = submit_program_default(user, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        populate_mailbox_from_program(prog_id, user, 2, 0)
    }

    // Puts message from `prog_id` for the `user` in mailbox and returns its id
    pub(super) fn populate_mailbox_from_program(
        prog_id: H256,
        user: AccountId,
        block_num: BlockNumber,
        program_nonce: u64,
    ) -> H256 {
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(user).into(),
            prog_id,
            Vec::new(),
            20_000_000, // `prog_id` program sends message in handle which sets gas limit to 10_000_000.
            0,
        ));
        run_to_block(block_num, None);

        {
            let expected_code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
            assert_eq!(
                common::get_program(prog_id)
                    .expect("program must exist")
                    .code_hash,
                sp_io::hashing::blake2_256(&expected_code).into(),
                "can invokle send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
            );
        }

        assert!(Mailbox::<Test>::contains_key(user));

        compute_program_message_id(prog_id.as_bytes(), program_nonce)
    }

    // Submits program with default options (salt, gas limit, value, payload)
    pub(super) fn submit_program_default(
        user: AccountId,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<H256> {
        let code = code_kind.to_bytes();
        let salt = DEFAULT_SALT.to_vec();
        // alternatively, get from last event
        let prog_id = generate_program_id(&code, &salt);
        GearPallet::<Test>::submit_program(
            Origin::signed(user).into(),
            code,
            salt,
            DEFAULT_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
        .map(|_| prog_id)
    }

    pub(super) fn generate_program_id(code: &[u8], salt: &[u8]) -> H256 {
        // TODO #512
        let mut data = Vec::new();
        code.encode_to(&mut data);
        salt.encode_to(&mut data);

        sp_io::hashing::blake2_256(&data[..]).into()
    }

    pub(super) fn send_default_message(from: AccountId, to: H256) -> DispatchResultWithPostInfo {
        GearPallet::<Test>::send_message(
            Origin::signed(from).into(),
            to,
            DEFAULT_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
    }

    pub(super) fn compute_user_message_id(payload: &[u8], global_nonce: u128) -> H256 {
        let mut id = payload.encode();
        id.extend_from_slice(&global_nonce.to_le_bytes());
        sp_io::hashing::blake2_256(&id).into()
    }

    pub(super) fn compute_program_message_id(program_id: &[u8], program_nonce: u64) -> H256 {
        let mut id = program_id.to_vec();
        id.extend_from_slice(&program_nonce.to_le_bytes());
        sp_io::hashing::blake2_256(&id).into()
    }

    #[derive(Debug, Copy, Clone)]
    pub(super) enum ProgramCodeKind<'a> {
        Default,
        Custom(&'a str),
        GreedyInit,
        OutgoingWithValueInHandle,
    }

    impl<'a> ProgramCodeKind<'a> {
        pub(super) fn to_bytes(self) -> Vec<u8> {
            let source = match self {
                ProgramCodeKind::Default => {
                    r#"
                    (module
                        (import "env" "memory" (memory 1))
                        (export "handle" (func $handle))
                        (export "init" (func $init))
                        (func $handle)
                        (func $init)
                    )"#
                }
                ProgramCodeKind::GreedyInit => {
                    // Initialization function for that program requires a lot of gas.
                    // So, providing `DEFAULT_GAS_LIMIT` will end up processing with
                    // "Gas limit exceeded" execution outcome error message.
                    r#"
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
                    )"#
                }
                ProgramCodeKind::OutgoingWithValueInHandle => {
                    // Sending message to USER_1 is hardcoded!
                    // Program sends message in handle which sets gas limit to 10_000_000 and value to 1000.
                    // [warning] - program payload data is inaccurate, don't make assumptions about it!
                    r#"
                    (module
                        (import "env" "gr_send" (func $send (param i32 i32 i32 i64 i32 i32)))
                        (import "env" "gr_source" (func $gr_source (param i32)))
                        (import "env" "memory" (memory 1))
                        (export "handle" (func $handle))
                        (export "init" (func $init))
                        (export "handle_reply" (func $handle_reply))
                        (func $handle
                            (local $msg_source i32)
                            (local $msg_val i32)
                            (i32.store offset=2
                                (get_local $msg_source)
                                (i32.const 1)
                            )
                            (i32.store offset=10
                                (get_local $msg_val)
                                (i32.const 1000)
                            )
                            (call $send (i32.const 2) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 10) (i32.const 40000))
                        )
                        (func $handle_reply)
                        (func $init)
                    )"#
                }
                ProgramCodeKind::Custom(code) => code,
            };

            wabt::Wat2Wasm::new()
                .validate(false)
                .convert(source)
                .expect("failed to parse module")
                .as_ref()
                .to_vec()
        }
    }
}
