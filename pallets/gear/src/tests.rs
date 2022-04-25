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

use codec::Encode;
use common::{self, CodeStorage, DAGBasedLedger, GasPrice as _, Origin as _};
use demo_distributor::{Request, WASM_BINARY};
use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};
use frame_support::{assert_noop, assert_ok};
use frame_system::Pallet as SystemPallet;
use gear_core::{code::Code, ids::CodeId};
use pallet_balances::{self, Pallet as BalancesPallet};

use super::{
    manager::HandleKind,
    mock::{
        calc_handle_gas_spent, new_test_ext, run_to_block, Event as MockEvent, Gear, GearProgram,
        Origin, System, Test, BLOCK_AUTHOR, LOW_BALANCE_USER, USER_1, USER_2, USER_3,
    },
    pallet, Config, DispatchOutcome, Error, Event, ExecutionResult, GasAllowance,
    GearProgramPallet, Mailbox, MessageInfo, Pallet as GearPallet, Reason,
};

use utils::*;

#[test]
fn unstoppable_block_execution_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let user_balance = BalancesPallet::<Test>::free_balance(USER_1) as u64;
        let executions_amount = 10;
        let balance_for_each_execution = user_balance / executions_amount;

        assert!(balance_for_each_execution < <Test as Config>::BlockGasLimit::get());

        let program_id = {
            let res = submit_program_default(USER_2, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, None);

        let (expected_burned_gas, _) =
            calc_handle_gas_spent(USER_1.into_origin(), program_id, EMPTY_PAYLOAD.to_vec());

        assert!(balance_for_each_execution > expected_burned_gas);

        for _ in 0..executions_amount {
            assert_ok!(GearPallet::<Test>::send_message(
                Origin::signed(USER_1),
                program_id,
                EMPTY_PAYLOAD.to_vec(),
                balance_for_each_execution,
                0,
            ));
        }

        let real_gas_to_burn = expected_burned_gas * executions_amount;

        assert!(balance_for_each_execution * executions_amount > real_gas_to_burn);

        run_to_block(3, Some(real_gas_to_burn));

        SystemPallet::<Test>::assert_last_event(
            Event::MessagesDequeued(executions_amount as u32).into(),
        );

        assert_eq!(GasAllowance::<Test>::get(), 0);

        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1) as u64,
            user_balance - real_gas_to_burn
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
                Origin::signed(USER_1),
                ProgramCodeKind::Default.to_bytes(),
                DEFAULT_SALT.to_vec(),
                EMPTY_PAYLOAD.to_vec(),
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
                Origin::signed(USER_1),
                ProgramCodeKind::Default.to_bytes(),
                DEFAULT_SALT.to_vec(),
                EMPTY_PAYLOAD.to_vec(),
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

        // No gas has been created initially
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);

        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_ok!(send_default_message(USER_1, program_id));

        // Balances check
        // Gas spends on sending 2 default messages (submit program and send message to program)
        let user1_potential_msgs_spends = GasPrice::gas_price(2 * DEFAULT_GAS_LIMIT);
        // User 1 has sent two messages
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );

        // Clear messages from the queue to refund unused gas
        run_to_block(2, None);

        // Checking that sending a message to a non-program address works as a value transfer
        let mail_value = 20_000;

        // Take note of up-to-date users balance
        let user1_initial_balance = BalancesPallet::<Test>::free_balance(USER_1);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            USER_2.into_origin(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            mail_value,
        ));
        let message_id = get_last_message_id();

        // Transfer of `mail_value` completed.
        // Gas limit is ignored for messages headed to a mailbox - no funds have been reserved.
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - mail_value
        );
        // The recipient has not received the funds, they are in the mailbox
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_2),
            user2_initial_balance
        );

        assert_ok!(GearPallet::<Test>::claim_value_from_mailbox(
            Origin::signed(USER_2),
            message_id
        ));

        // The recipient has received funds
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_2),
            user2_initial_balance + mail_value
        );

        // Ensure the message didn't burn any gas (i.e. never went through processing pipeline)
        let remaining_weight = 100_000;
        run_to_block(3, Some(remaining_weight));

        // Messages were sent by user 1 only
        let actual_gas_burned = remaining_weight - GasAllowance::<Test>::get();
        assert_eq!(actual_gas_burned, 0);

        // Ensure all created imbalances along the way cancel each other
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);
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
            Error::<Test>::ProgramIsTerminated
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

        // Because destination is user, no gas will be reserved
        assert!(matches!(
            Mailbox::<Test>::remove_all(None),
            sp_io::KillStorageResult::AllRemoved(_)
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(LOW_BALANCE_USER),
            USER_1.into_origin(),
            EMPTY_PAYLOAD.to_vec(),
            1000,
            1
        ));
        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() > 0);

        // Gas limit too high
        let block_gas_limit = <Test as pallet::Config>::BlockGasLimit::get();
        assert_noop!(
            GearPallet::<Test>::send_message(
                Origin::signed(USER_1),
                program_id,
                EMPTY_PAYLOAD.to_vec(),
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
        let gas_spent = GasPrice::gas_price(
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

        // Initial value in all gas trees is 0
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);

        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            huge_send_message_gas_limit,
            0
        ));
        // Spends for submit program with default gas limit and sending default message with a huge gas limit
        let user1_potential_msgs_spends =
            GasPrice::gas_price(DEFAULT_GAS_LIMIT + huge_send_message_gas_limit);
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );
        assert_eq!(
            BalancesPallet::<Test>::reserved_balance(USER_1),
            (DEFAULT_GAS_LIMIT + huge_send_message_gas_limit) as _,
        );

        run_to_block(2, None);
        let user1_actual_msgs_spends = GasPrice::gas_price(
            <Test as pallet::Config>::BlockGasLimit::get() - GasAllowance::<Test>::get(),
        );
        assert!(user1_potential_msgs_spends > user1_actual_msgs_spends);
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends
        );

        // All created gas cancels out
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);
    })
}

#[cfg(unix)]
#[test]
fn lazy_pages() {
    use gear_core::memory::{wasm_pages_to_pages_set, PageNumber, WasmPageNumber};
    use gear_runtime_interface as gear_ri;
    use std::collections::BTreeSet;

    // This test access different pages in linear wasm memory
    // and check that lazy-pages (see gear-lazy-pages) works correct:
    // For each page, which has been loaded from storage <=> page has been accessed.
    let wat = r#"
	(module
		(import "env" "memory" (memory 1))
        (import "env" "alloc" (func $alloc (param i32) (result i32)))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $init
            ;; allocate 9 pages in init, so mem will contain 10 pages
            i32.const 0x0
            i32.const 0x9
            call $alloc
            i32.store
        )
        (func $handle
            ;; write access wasm page 0
            i32.const 0x0
            i32.const 0x42
            i32.store

            ;; write access wasm page 2
            ;; but here we access two native pages, if native page is less then 16kiB
            i32.const 0x23ffe
            i32.const 0x42
            i32.store

            ;; read access wasm page 5
            i32.const 0x0
            i32.const 0x50000
            i32.load
            i32.store

            ;; write access wasm pages 8 and 9 by one store
            i32.const 0x8fffc
            i64.const 0xffffffffffffffff
            i64.store
		)
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = {
            let code = ProgramCodeKind::Custom(wat).to_bytes();
            let salt = DEFAULT_SALT.to_vec();
            let prog_id = generate_program_id(&code, &salt);
            let res = GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                code,
                salt,
                EMPTY_PAYLOAD.to_vec(),
                5_000_000,
                0,
            )
            .map(|_| prog_id);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, Some(10_000_000));
        log::debug!("submit done {:?}", pid);
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());

        let res = GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000,
            100,
        );
        log::debug!("res = {:?}", res);
        assert_ok!(res);

        run_to_block(3, Some(10_000_000));

        // Dirty hack: lazy pages info is stored in thread local static variables,
        // so after contract execution lazy-pages information
        // remains correct and we can use it here.
        let lazy_pages: BTreeSet<PageNumber> = gear_ri::gear_ri::get_wasm_lazy_pages_numbers()
            .iter()
            .map(|p| PageNumber(*p))
            .collect();
        let released_pages: BTreeSet<PageNumber> = gear_ri::gear_ri::get_released_pages()
            .iter()
            .map(|p| PageNumber(*p))
            .collect();

        // Checks that released pages + lazy pages == all pages
        let all_pages = {
            let all_wasm_pages: BTreeSet<WasmPageNumber> = (0..10u32).map(WasmPageNumber).collect();
            wasm_pages_to_pages_set(all_wasm_pages.iter())
        };
        let mut res_pages = lazy_pages;
        res_pages.extend(released_pages.iter());

        assert_eq!(res_pages, all_pages);

        // checks accessed pages set
        let native_size = page_size::get();
        let mut expected_accessed = BTreeSet::new();

        let page_to_accessed = |p: u32| {
            if native_size > PageNumber::size() {
                let x = (native_size / PageNumber::size()) as u32;
                (p / x) * x..=(p / x) * x + x - 1
            } else {
                p..=p
            }
        };

        // accessed from 0 wasm page:
        expected_accessed.extend(page_to_accessed(0));

        // accessed from 2 wasm page, can be seweral gear and native pages:
        let first_page = (0x23ffe / PageNumber::size()) as u32;
        let second_page = (0x24001 / PageNumber::size()) as u32;
        expected_accessed.extend(page_to_accessed(first_page));
        expected_accessed.extend(page_to_accessed(second_page));

        // accessed from 5 wasm page:
        expected_accessed.extend(page_to_accessed((0x50000 / PageNumber::size()) as u32));

        // accessed from 8 and 9 wasm pages, must be seweral gear pages:
        let first_page = (0x8fffc / PageNumber::size()) as u32;
        let second_page = (0x90003 / PageNumber::size()) as u32;
        expected_accessed.extend(page_to_accessed(first_page));
        expected_accessed.extend(page_to_accessed(second_page));

        assert_eq!(
            released_pages,
            expected_accessed.into_iter().map(PageNumber).collect()
        );
    });
}

#[test]
fn block_gas_limit_works() {
    // Same as `ProgramCodeKind::OutgoingWithValueInHandle`, but without value sending
    let wat1 = r#"
    (module
        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32) (result i32)))
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
                (i32.const 0)
            )
            (call $send (i32.const 2) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 10) (i32.const 40000))
            (if
                (then unreachable)
                (else)
            )
        )
        (func $handle_reply)
        (func $init)
    )"#;

    // Same as `ProgramCodeKind::GreedyInit`, but greedy handle
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
        let remaining_weight = 791822625 + 6228260 - 1; // calc gas pid1 + pid2 - 1

        // Submit programs and get their ids
        let pid1 = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Custom(wat1));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        let pid2 = {
            let res = submit_program_default(USER_1, ProgramCodeKind::Custom(wat2));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, Some(remaining_weight));
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());

        // Count gas needed to process programs with default payload
        let (expected_gas_msg_to_pid1, _) =
            calc_handle_gas_spent(USER_1.into_origin(), pid1, EMPTY_PAYLOAD.to_vec());
        let (expected_gas_msg_to_pid2, _) =
            calc_handle_gas_spent(USER_1.into_origin(), pid2, EMPTY_PAYLOAD.to_vec());

        // TrapInHandle code kind is used because processing default payload in its
        // context requires such an amount of gas, that the following assertion can be passed.
        assert!(expected_gas_msg_to_pid1 + expected_gas_msg_to_pid2 > remaining_weight);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            100
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            100
        ));

        run_to_block(3, Some(remaining_weight));
        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());

        // Run to the next block to reset the gas limit
        run_to_block(4, Some(remaining_weight));

        // Add more messages to queue
        // Total `gas_limit` of three messages (2 to pid1 and 1 to pid2) exceeds the block gas limit
        assert!(remaining_weight < 2 * expected_gas_msg_to_pid1 + remaining_weight);
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            200
        ));
        let msg1 = get_last_message_id();

        let msg2_gas = remaining_weight;
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            msg2_gas,
            100
        ));
        let _msg2 = get_last_message_id();

        let msg3_gas = expected_gas_msg_to_pid1;
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            200
        ));
        let _msg3 = get_last_message_id();

        // Try to process 3 messages
        run_to_block(5, Some(remaining_weight));

        // Message #1 is dequeued and processed.
        // Message #2 tried to execute, but exceed gas_allowance is re-queued at the top.
        // Message #3 stays in the queue.
        //
        // | 1 |        | 2 |
        // | 2 |  ===>  | 3 |
        // | 3 |        |   |

        SystemPallet::<Test>::assert_has_event(
            Event::MessageDispatched(DispatchOutcome {
                message_id: msg1.into_origin(),
                outcome: ExecutionResult::Failure(
                    b"Gas limit exceeded while trying to send message".to_vec(),
                ),
            })
            .into(),
        );

        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(1).into());

        // Equals 0 due to trying execution of msg2.
        assert_eq!(GasAllowance::<Test>::get(), 0);

        let real_gas_to_burn = expected_gas_msg_to_pid1 + expected_gas_msg_to_pid2;
        let last_block_allowance = real_gas_to_burn + 1;

        // Try to process 2 messages
        run_to_block(6, Some(last_block_allowance));

        assert!(last_block_allowance < msg2_gas + msg3_gas);

        // Message #2 gas limit exceeds the remaining allowance, but got processed.
        // Message #3 same suits that block.
        //
        // | 2 |        |   |
        // | 3 |  ===>  |   |
        // |   |        |   |

        SystemPallet::<Test>::assert_last_event(Event::MessagesDequeued(2).into());
        assert_eq!(
            GasAllowance::<Test>::get(),
            last_block_allowance - real_gas_to_burn
        );
    });
}

#[test]
fn mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);

        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_1);

        // Ensure that all the gas has been returned to the sender upon messages processing
        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

        let mailbox_message = {
            let res = GearPallet::<Test>::remove_from_mailbox(USER_1.into_origin(), reply_to_id);
            assert!(res.is_some());
            res.expect("was asserted previously")
        };

        assert_eq!(mailbox_message.id().into_origin(), reply_to_id);

        // Gas limit should have been ignored by the code that puts a message into a mailbox
        assert_eq!(mailbox_message.value(), 1000);

        // Gas is not passed to mailboxed messages and should have been all spent by now
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);
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

            let event = match SystemPallet::<Test>::events()
                .last()
                .map(|r| r.event.clone())
            {
                Some(MockEvent::Gear(e)) => e,
                _ => unreachable!("Should be one Gear event"),
            };

            run_to_block(next_block, None);

            let msg_info = match event {
                Event::InitMessageEnqueued(info) => info,
                _ => unreachable!("expect Event::InitMessageEnqueued"),
            };

            SystemPallet::<Test>::assert_has_event(if is_failing {
                Event::InitFailure(
                    msg_info,
                    Reason::Dispatch(trap_explanation.as_bytes().to_vec()),
                )
                .into()
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

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        run_to_block(2, None);

        assert!(Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        // Submitting second program, which fails on initialization, therefore is deleted
        let program_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        run_to_block(3, None);

        assert!(!Gear::is_initialized(program_id));
        // while at the same time is terminated
        assert!(Gear::is_terminated(program_id));
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
        let mut next_block = 2;
        let tests = [
            // Code, init failure reason, handle succeed flag
            (ProgramCodeKind::Default, None, true),
            (
                ProgramCodeKind::GreedyInit,
                Some("Gas limit exceeded".as_bytes().to_vec()),
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

            let message_id = get_last_message_id();

            let init_msg_info = MessageInfo {
                program_id,
                message_id,
                origin: USER_1.into_origin(),
            };

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
                    Error::<Test>::ProgramIsTerminated
                );
                continue;
            }

            SystemPallet::<Test>::assert_has_event(Event::InitSuccess(init_msg_info).into());

            // Messages to fully-initialized programs are accepted
            assert_ok!(send_default_message(USER_1, program_id));

            let message_id = get_last_message_id();

            let dispatch_msg_info = MessageInfo {
                program_id,
                message_id,
                origin: USER_1.into_origin(),
            };

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

        let prog_id = generate_program_id(
            &ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.as_ref(),
        );

        // Top up program's account balance by 2000 to allow user claim 1000 from mailbox
        assert_ok!(
            <BalancesPallet::<Test> as frame_support::traits::Currency<_>>::transfer(
                &USER_1,
                &AccountId::from_origin(prog_id),
                2000,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            1000, // `prog_id` sent message with value of 1000 (see program code)
        ));
        let message_id = get_last_message_id();

        // global nonce is 2 before sending reply message
        // `submit_program` and `send_message` messages were sent before in `setup_mailbox_test_state`
        let expected_reply_message_id = message_id;

        let event = match SystemPallet::<Test>::events()
            .last()
            .map(|r| r.event.clone())
        {
            Some(MockEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        let MessageInfo {
            message_id: actual_reply_message_id,
            ..
        } = match event {
            Event::DispatchMessageEnqueued(info) => info,
            _ => unreachable!("expect Event::DispatchMessageEnqueued"),
        };

        assert_eq!(expected_reply_message_id, actual_reply_message_id);
    })
}

#[test]
fn send_reply_failure_to_claim_from_mailbox() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Expecting error as long as the user doesn't have messages in mailbox
        assert_noop!(
            GearPallet::<Test>::send_reply(
                Origin::signed(USER_1),
                5.into_origin(), // non existent `reply_to_id`
                EMPTY_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                0
            ),
            Error::<Test>::NoMessageInMailbox
        );

        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        if let common::Program::Terminated =
            common::get_program(prog_id).expect("Failed to get program from storage")
        {
            panic!("Program is terminated!");
        };

        populate_mailbox_from_program(prog_id, USER_1, 2, 2_000_000_000, 0);

        // Program didn't have enough balance, so it's message produces trap
        // (and following system reply with error to USER_1 mailbox)
        assert_eq!(Mailbox::<Test>::iter_prefix(USER_1).count(), 1);

        let message = Mailbox::<Test>::iter_prefix_values(USER_1)
            .next()
            .expect("Checked above");

        assert!(matches!(message.reply(), Some((_, 1))));
    })
}

#[test]
fn send_reply_value_claiming_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // This value is actually a constants in WAT. Alternatively can be read from Mailbox.
        let locked_value = 1000;

        // Top up program's account so it could send value in message
        // When program sends message, message value (if not 0) is reserved.
        // If value can't be reserved, message is skipped.
        let send_to_program_amount = locked_value * 2;
        assert_ok!(
            <BalancesPallet::<Test> as frame_support::traits::Currency<_>>::transfer(
                &USER_1,
                &AccountId::from_origin(prog_id),
                send_to_program_amount,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );

        let mut next_block = 2;

        let user_messages_data = [
            // gas limit, value
            (1_000_000, 100),
            (20_000_000, 2000),
        ];
        for (gas_limit_to_reply, value_to_reply) in user_messages_data {
            let reply_to_id =
                populate_mailbox_from_program(prog_id, USER_1, next_block, 2_000_000_000, 0);

            next_block += 1;

            assert!(Mailbox::<Test>::iter_prefix(USER_1).count() > 0);

            let user_balance = BalancesPallet::<Test>::free_balance(USER_1);
            assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

            assert_ok!(GearPallet::<Test>::send_reply(
                Origin::signed(USER_1),
                reply_to_id,
                EMPTY_PAYLOAD.to_vec(),
                gas_limit_to_reply,
                value_to_reply,
            ));

            let user_expected_balance =
                user_balance - value_to_reply - GasPrice::gas_price(gas_limit_to_reply)
                    + locked_value;

            assert_eq!(
                BalancesPallet::<Test>::free_balance(USER_1),
                user_expected_balance
            );
            assert_eq!(
                BalancesPallet::<Test>::reserved_balance(USER_1),
                GasPrice::gas_price(gas_limit_to_reply) + value_to_reply
            );
        }
    })
}

// user 1 sends to prog msg
// prog send to user 1 msg to mailbox
// user 1 claims it from mailbox

#[test]
fn claim_value_from_mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let sender_balance = BalancesPallet::<Test>::free_balance(USER_2);
        let claimer_balance = BalancesPallet::<Test>::free_balance(USER_1);

        let gas_sent = 1_000_000_000;
        let value_sent = 1000;

        let prog_id = {
            let res = submit_program_default(USER_3, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        increase_prog_balance_for_mailbox_test(USER_3, prog_id);
        let reply_to_id = populate_mailbox_from_program(prog_id, USER_2, 2, gas_sent, value_sent);
        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() > 0);

        let (gas_burned, _) =
            calc_handle_gas_spent(USER_1.into_origin(), prog_id, EMPTY_PAYLOAD.to_vec());
        let gas_burned = GasPrice::gas_price(gas_burned);

        run_to_block(3, None);

        assert_ok!(GearPallet::<Test>::claim_value_from_mailbox(
            Origin::signed(USER_1),
            reply_to_id,
        ));

        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);
        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_2), 0);

        let expected_claimer_balance = claimer_balance + value_sent;
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            expected_claimer_balance
        );

        // Gas left returns to sender from consuming of value tree while claiming.
        let expected_sender_balance = sender_balance - value_sent - gas_burned;
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_2),
            expected_sender_balance
        );

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
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            0,
        ));

        run_to_block(2, None);

        // At this point there is a message in USER_1's mailbox, however, since messages in
        // mailbox are stripped of the `gas_limit`, the respective gas tree has been consumed
        // and the value unreserved back to the original sender (USER_1)
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

        // Initial value in all gas trees is 0
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);

        let program_id = generate_program_id(WASM_BINARY, DEFAULT_SALT);

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            Request::Receive(10).encode(),
            20_000_000,
            0,
        ));

        run_to_block(3, None);

        // Despite some messages are still in the mailbox all gas locked in value trees
        // has been refunded to the sender so the free balances should add up
        let final_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);

        assert_eq!(initial_balance, final_balance);

        // All gas cancelled out in the end
        assert_eq!(<Test as Config>::GasHandler::total_supply(), 0);
    });
}

#[test]
fn test_code_submission_pass() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = generate_code_hash(&code).into();
        let code_id = CodeId::from_origin(code_hash);

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            code.clone()
        ));

        let saved_code = <Test as Config>::CodeStorage::get_code(code_id);

        let schedule = <Test as Config>::Schedule::get();
        let code = Code::try_new(code, schedule.instruction_weights.version, |module| {
            schedule.rules(module)
        })
        .expect("Error creating Code");
        assert_eq!(saved_code.unwrap().code(), code.code());

        let expected_meta = Some(common::CodeMetadata::new(USER_1.into_origin(), 1));
        let actual_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
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
            GearPallet::<Test>::submit_code(Origin::signed(USER_2), code),
            Error::<Test>::CodeAlreadyExists,
        );
    })
}

#[test]
fn test_code_is_not_submitted_twice_after_program_submission() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = generate_code_hash(&code).into();

        // First submit program, which will set code and metadata
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        SystemPallet::<Test>::assert_has_event(Event::CodeSaved(code_hash).into());
        assert!(<Test as Config>::CodeStorage::exists(CodeId::from_origin(
            code_hash
        )));

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
        let code_hash = generate_code_hash(&code).into();
        let code_id = CodeId::from_origin(code_hash);

        // First submit code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            code.clone()
        ));
        let expected_code_saved_events = 1;
        let expected_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
        assert!(expected_meta.is_some());

        // Submit program from another origin. Should not change meta or code.
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        let actual_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
        let actual_code_saved_events = SystemPallet::<Test>::events()
            .iter()
            .filter(|e| matches!(e.event, MockEvent::Gear(Event::CodeSaved(_))))
            .count();

        assert_eq!(expected_meta, actual_meta);
        assert_eq!(expected_code_saved_events, actual_code_saved_events);
    })
}

#[test]
fn messages_to_uninitialized_program_wait() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        run_to_block(2, None);

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(1),
            program_id,
            vec![],
            10_000u64,
            0u128
        ));

        run_to_block(3, None);

        assert_eq!(common::waiting_init_take_messages(program_id).len(), 1);
    })
}

#[test]
fn uninitialized_program_should_accept_replies() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            5_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));

        run_to_block(2, None);

        // there should be one message for the program author
        let mut mailbox_iter = Mailbox::<Test>::iter_prefix_values(USER_1);
        let message_id = mailbox_iter
            .next()
            .expect("Should have msg.")
            .id()
            .into_origin();

        assert!(mailbox_iter.next().is_none());

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            5_000_000_000u64,
            0,
        ));

        run_to_block(3, None);

        assert!(Gear::is_initialized(program_id));
    })
}

#[test]
fn defer_program_initialization() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            5_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        let message_id = Mailbox::<Test>::iter_prefix_values(USER_1)
            .next()
            .expect("should be one message for the program author")
            .id()
            .into_origin();

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            5_000_000_000u64,
            0,
        ));

        run_to_block(3, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            vec![],
            5_000_000_000u64,
            0u128
        ));

        run_to_block(4, None);

        assert_eq!(Mailbox::<Test>::iter_prefix(USER_1).count(), 1);

        let message = Mailbox::<Test>::iter_prefix_values(USER_1)
            .next()
            .expect("Message not found");

        assert_eq!(message.payload().to_vec(), b"Hello, world!".encode());
    })
}

#[test]
fn wake_messages_after_program_inited() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            5_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        // While program is not inited all messages addressed to it are waiting.
        // There could be dozens of them.
        let n = 10;
        for _ in 0..n {
            assert_ok!(GearPallet::<Test>::send_message(
                Origin::signed(USER_3),
                program_id,
                vec![],
                2_000_000_000u64,
                0u128
            ));
        }

        run_to_block(3, None);

        let message_id = Mailbox::<Test>::iter_prefix(USER_1)
            .next()
            .map(|(k, _msg)| k.into_origin())
            .expect("Message id should be");

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            20_000_000_000u64,
            0,
        ));

        run_to_block(20, None);

        let actual_n = Mailbox::<Test>::iter_prefix_values(USER_3).fold(0usize, |i, m| {
            assert_eq!(m.payload().to_vec(), b"Hello, world!".encode());
            i + 1
        });

        assert_eq!(actual_n, n);
    })
}

#[test]
fn test_message_processing_for_non_existing_destination() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = submit_program_default(USER_1, ProgramCodeKind::GreedyInit).expect("todo");
        let code_hash =
            generate_code_hash(ProgramCodeKind::GreedyInit.to_bytes().as_slice()).into();
        let user_balance_before = BalancesPallet::<Test>::free_balance(USER_1);

        // After running, first message will end up with init failure, so destination address won't exist.
        // However, message to that non existing address will be in message queue. So, we test that this message is not executed.
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000,
            100
        ));
        let skipped_message_id = get_last_message_id();
        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() == 0);

        run_to_block(2, None);
        // system reply message
        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() > 0);

        let user_balance_after = BalancesPallet::<Test>::free_balance(USER_1);
        assert_eq!(user_balance_before, user_balance_after);

        SystemPallet::<Test>::assert_has_event(
            Event::MessageNotExecuted(skipped_message_id).into(),
        );

        assert!(Gear::is_terminated(program_id));
        assert!(<Test as Config>::CodeStorage::exists(CodeId::from_origin(
            code_hash
        )));
    })
}

#[test]
fn exit_init() {
    use demo_exit_init::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code.clone(),
            vec![],
            Vec::new(),
            50_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_terminated(program_id));
        assert!(!Gear::is_initialized(program_id));

        let actual_n = Mailbox::<Test>::iter_prefix_values(USER_1).fold(0usize, |i, _| i + 1);

        assert_eq!(actual_n, 0);

        // Program is not removed and can't be submitted again
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                code,
                vec![],
                Vec::new(),
                1_000_000_000u64,
                0u128
            ),
            Error::<Test>::ProgramAlreadyExists,
        );
    })
}

#[test]
fn test_create_program_no_code_hash() {
    let non_constructable_wat = r#"
    (module)
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let factory_code = PROGRAM_FACTORY_WASM_BINARY;
        let factory_id = generate_program_id(factory_code, DEFAULT_SALT);

        let valid_code_hash = generate_code_hash(ProgramCodeKind::Default.to_bytes().as_slice());
        let invalid_prog_code_kind = ProgramCodeKind::Custom(non_constructable_wat);
        let invalid_prog_code_hash =
            generate_code_hash(invalid_prog_code_kind.to_bytes().as_slice());

        // Creating factory
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            5_000_000_000,
            0,
        ));

        // Try to create a program with non existing code hash
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            5_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Init and dispatch messages from the contract are dequeued, but not executed
        // 2 error replies are generated, and executed
        check_dequeued(4 + 2); // +2 for submit_program/send_messages
        check_dispatched(2 + 1); // +1 for send_messages
        check_init_success(1); // 1 for submitting factory

        SystemPallet::<Test>::reset_events();

        // Try to create multiple programs with non existing code hash
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (valid_code_hash, b"salt1".to_vec(), 10_000),
                (valid_code_hash, b"salt2".to_vec(), 10_000),
                (valid_code_hash, b"salt3".to_vec(), 10_000),
            ])
            .encode(),
            10_000_000_000,
            0,
        ));
        run_to_block(3, None);
        // Init and dispatch messages from the contract are dequeued, but not executed
        // 2 error replies are generated, and executed
        check_dequeued(12 + 1); // +1 for send_message
        check_dispatched(6 + 1); // +1 for send_message
        check_init_success(0);

        assert_noop!(
            GearPallet::<Test>::submit_code(
                Origin::signed(USER_1),
                invalid_prog_code_kind.to_bytes(),
            ),
            Error::<Test>::FailedToConstructProgram,
        );

        SystemPallet::<Test>::reset_events();

        // Try to create with invalid code hash
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (invalid_prog_code_hash, b"salt1".to_vec(), 10_000),
                (invalid_prog_code_hash, b"salt2".to_vec(), 10_000),
                (invalid_prog_code_hash, b"salt3".to_vec(), 10_000),
            ])
            .encode(),
            10_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // Init and dispatch messages from the contract are dequeued, but not executed
        // 2 error replies are generated, and executed
        check_dequeued(12 + 1); // +1 for send_message
        check_dispatched(6 + 1); // +1 for send_message
        check_init_success(0);
    });
}

#[test]
fn test_create_program_simple() {
    init_logger();
    new_test_ext().execute_with(|| {
        let factory_code = PROGRAM_FACTORY_WASM_BINARY;
        let factory_id = generate_program_id(factory_code, DEFAULT_SALT);
        let child_code = ProgramCodeKind::Default.to_bytes();
        let child_code_hash = generate_code_hash(&child_code);

        // Submit the code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            child_code,
        ));

        // Creating factory
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            4_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Test create one successful in init program
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(3, None);

        // Test create one failing in init program
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(
                vec![(child_code_hash, b"some_data".to_vec(), 3000)] // too little gas
            )
            .encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // First extrinsic call with successful program creation dequeues and executes init and dispatch messages
        // Second extrinsic is failing one, for each message it generates replies, which are executed (4 dequeued, 2 dispatched)
        check_dequeued(6 + 3); // +3 for extrinsics
        check_dispatched(3 + 2); // +2 for extrinsics
        check_init_success(1 + 1); // +1 for submitting factory

        SystemPallet::<Test>::reset_events();

        // Create multiple successful init programs
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt1".to_vec(), 10_000),
                (child_code_hash, b"salt2".to_vec(), 10_000),
            ])
            .encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(5, None);

        // Create multiple successful init programs
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt3".to_vec(), 3000), // too little gas
                (child_code_hash, b"salt4".to_vec(), 3000), // too little gas
            ])
            .encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(6, None);

        check_dequeued(12 + 2); // +2 for extrinsics
        check_dispatched(6 + 2); // +2 for extrinsics
        check_init_success(2);
    })
}

#[test]
fn test_create_program_duplicate() {
    init_logger();
    new_test_ext().execute_with(|| {
        let factory_code = PROGRAM_FACTORY_WASM_BINARY;
        let factory_id = generate_program_id(factory_code, DEFAULT_SALT);
        let child_code = ProgramCodeKind::Default.to_bytes();
        let child_code_hash = generate_code_hash(&child_code);

        // Submit the code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            child_code.clone(),
        ));

        // Creating factory
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            2_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // User creates a program
        assert_ok!(submit_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(3, None);

        // Program tries to create the same
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, DEFAULT_SALT.to_vec(), 100_000),])
                .encode(),
            2_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // When duplicate try happens, init is not executed, a reply is generated and executed (+2 dequeued, +1 dispatched)
        // Concerning dispatch message, it is executed, because destination exists (+1 dispatched, +1 dequeued)
        check_dequeued(3 + 3); // +3 from extrinsics (2 submit_program, 1 send_message)
        check_dispatched(2 + 1); // +1 from extrinsic (send_message)
        check_init_success(2); // +2 from extrinsics (2 submit_program)

        SystemPallet::<Test>::reset_events();

        // Create a new program from program
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 100_000),]).encode(),
            2_000_000_000,
            0,
        ));
        run_to_block(5, None);

        // Try to create the same
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 100_000),]).encode(),
            2_000_000_000,
            0,
        ));
        run_to_block(6, None);

        // First call successfully creates a program and sends a messages to it (+2 dequeued, +1 dispatched)
        // Second call will not cause init message execution, but a reply will be generated (+2 dequeued, +1 dispatched)
        // Handle message from the second call will be executed (addressed for existing destination) (+1 dequeued, +1 dispatched)
        check_dequeued(5 + 2); // +2 from extrinsics (send_message)
        check_dispatched(3 + 2); // +2 from extrinsics (send_message)
        check_init_success(1);

        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                child_code,
                b"salt1".to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                2_000_000_000,
                0,
            ),
            Error::<Test>::ProgramAlreadyExists,
        );
    });
}

#[test]
fn test_create_program_duplicate_in_one_execution() {
    init_logger();
    new_test_ext().execute_with(|| {
        let factory_code = PROGRAM_FACTORY_WASM_BINARY;
        let factory_id = generate_program_id(factory_code, DEFAULT_SALT);

        let child_code = ProgramCodeKind::Default.to_bytes();
        let child_code_hash = generate_code_hash(&child_code);

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_2),
            child_code,
        ));

        // Creating factory
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            2_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Try to create duplicate during one execution
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt1".to_vec(), 10_000), // could be successful init
                (child_code_hash, b"salt1".to_vec(), 10_000), // duplicate
            ])
            .encode(),
            2_000_000_000,
            0,
        ));

        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() == 0);

        run_to_block(3, None);

        // Duplicate init fails the call and returns error reply to the caller, which is USER_1.
        // State roll-back is performed.
        check_dequeued(2); // 2 for extrinsics
        check_dispatched(1); // 1 for send_message
        check_init_success(1); // 1 for creating a factory

        assert!(Mailbox::<Test>::iter_prefix(USER_1).count() > 0);

        SystemPallet::<Test>::reset_events();

        // Successful child creation
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 100_000),]).encode(),
            2_000_000_000,
            0,
        ));

        run_to_block(4, None);

        check_dequeued(2 + 1); // 1 for extrinsics
        check_dispatched(1 + 1); // 1 for send_message
        check_init_success(1);
    });
}

#[test]
fn test_create_program_miscellaneous() {
    // Same as ProgramCodeKind::Default, but has a different hash (init and handle method are swapped)
    // So code hash is different
    let child2_wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $init)
        (func $handle)
    )
    "#;
    init_logger();
    new_test_ext().execute_with(|| {
        let factory_code = PROGRAM_FACTORY_WASM_BINARY;
        let factory_id = generate_program_id(factory_code, DEFAULT_SALT);

        let child1_code = ProgramCodeKind::Default.to_bytes();
        let child2_code = ProgramCodeKind::Custom(child2_wat).to_bytes();

        let child1_code_hash = generate_code_hash(&child1_code);
        let child2_code_hash = generate_code_hash(&child2_code);

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_2),
            child1_code,
        ));
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_2),
            child2_code,
        ));

        // Creating factory
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            5_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child1_code_hash, b"salt1".to_vec(), 10_000),
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child1_code_hash, b"salt2".to_vec(), 1000),
            ])
            .encode(),
            5_000_000_000,
            0,
        ));

        run_to_block(3, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 3000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt2".to_vec(), 10_000),
            ])
            .encode(),
            5_000_000_000,
            0,
        ));

        run_to_block(4, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![
                // duplicate in the next block: init not executed, nor the handle (because destination is terminated), replies are generated (+4 dequeue, +2 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 10_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt3".to_vec(), 10_000),
            ])
            .encode(),
            5_000_000_000,
            0,
        ));

        run_to_block(5, None);

        check_dequeued(18 + 4); // +4 for 3 send_message calls and 1 submit_program call
        check_dispatched(9 + 3); // +3 for send_message calls
        check_init_success(3 + 1); // +1 for submitting factory
    });
}

#[test]
fn exit_handle() {
    use demo_exit_handle::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        let code_hash = generate_code_hash(&code).into();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code.clone(),
            vec![],
            Vec::new(),
            400_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(program_id));

        // An expensive operation since "gr_exit" removes all program pages from storage.
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            vec![],
            50_000_000_000u64,
            0u128
        ));

        run_to_block(3, None);

        assert!(Gear::is_terminated(program_id));

        let actual_n = Mailbox::<Test>::iter_prefix_values(USER_3).fold(0usize, |i, _| i + 1);

        assert_eq!(actual_n, 0);

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_terminated(program_id));

        assert!(<Test as Config>::CodeStorage::exists(CodeId::from_origin(
            code_hash
        )));

        // Program is not removed and can't be submitted again
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                code,
                vec![],
                Vec::new(),
                200_000_000u64,
                0u128
            ),
            Error::<Test>::ProgramAlreadyExists,
        );
    })
}

#[test]
fn paused_program_keeps_id() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code.clone(),
            vec![],
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_3),
                code,
                vec![],
                Vec::new(),
                2_000_000_000u64,
                0u128
            ),
            Error::<Test>::ProgramAlreadyExists
        );

        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_terminated(program_id));
    })
}

#[test]
fn messages_to_paused_program_skipped() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            vec![],
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        let before_balance = BalancesPallet::<Test>::free_balance(USER_3);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_3),
            program_id,
            vec![],
            1_000_000_000u64,
            1000u128
        ));

        run_to_block(3, None);

        assert_eq!(before_balance, BalancesPallet::<Test>::free_balance(USER_3));
    })
}

#[test]
fn replies_to_paused_program_skipped() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            vec![],
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(program_id));

        run_to_block(3, None);

        let message_id = Mailbox::<Test>::iter_prefix(USER_1)
            .next()
            .map(|(k, _msg)| k.into_origin())
            .expect("Message id should be");

        let before_balance = BalancesPallet::<Test>::free_balance(USER_1);

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            50_000_000u64,
            1000u128,
        ));

        run_to_block(4, None);

        assert_eq!(before_balance, BalancesPallet::<Test>::free_balance(USER_1));
    })
}

#[test]
fn program_messages_to_paused_program_skipped() {
    use demo_init_wait::WASM_BINARY;
    use demo_proxy::{InputArgs, WASM_BINARY as PROXY_WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            vec![],
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));

        let paused_program_id = utils::get_last_program_id();

        let code = PROXY_WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_3),
            code,
            vec![],
            InputArgs {
                destination: paused_program_id.into()
            }
            .encode(),
            2_000_000_000u64,
            1_000u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert_ok!(GearProgram::pause_program(paused_program_id));

        run_to_block(3, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_3),
            program_id,
            vec![],
            2_000_000_000u64,
            1_000u128
        ));

        run_to_block(4, None);

        assert_eq!(
            2_000u128,
            BalancesPallet::<Test>::free_balance(
                &<utils::AccountId as common::Origin>::from_origin(program_id)
            )
        );
    })
}

#[test]
fn resume_program_works() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            vec![],
            Vec::new(),
            5_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        let message_id = Mailbox::<Test>::iter_prefix(USER_1)
            .next()
            .map(|(k, _msg)| k.into_origin())
            .expect("Message id should be");

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            2_000_000_000u64,
            1_000u128,
        ));

        run_to_block(3, None);

        let program = match common::get_program(program_id).expect("program exists") {
            common::Program::Active(p) => p,
            _ => unreachable!(),
        };
        let memory_pages = common::get_program_pages(program_id, program.persistent_pages)
            .expect("program exists, so do pages");

        assert_ok!(GearProgram::pause_program(program_id));

        run_to_block(4, None);

        assert_ok!(GearProgramPallet::<Test>::resume_program(
            Origin::signed(USER_3),
            program_id,
            memory_pages,
            Default::default(),
            50_000u128
        ));

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_3),
            program_id,
            vec![],
            2_000_000_000u64,
            0u128
        ));

        run_to_block(5, None);

        let actual_n = Mailbox::<Test>::iter_prefix_values(USER_3).fold(0usize, |i, m| {
            assert_eq!(m.payload(), b"Hello, world!".encode());
            i + 1
        });

        assert_eq!(actual_n, 1);
    })
}

#[test]
fn gas_spent_vs_balance() {
    use demo_btree::{Request, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = BalancesPallet::<Test>::free_balance(USER_1);

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            0,
        ));

        let prog_id = utils::get_last_program_id();

        run_to_block(2, None);

        let balance_after_init = BalancesPallet::<Test>::free_balance(USER_1);

        let request = Request::Clear.encode();
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            prog_id,
            request.clone(),
            100_000_000,
            0
        ));

        run_to_block(3, None);

        let balance_after_handle = BalancesPallet::<Test>::free_balance(USER_1);

        let init_gas_spent = Gear::get_gas_spent(
            USER_1.into_origin(),
            HandleKind::Init(WASM_BINARY.to_vec()),
            EMPTY_PAYLOAD.to_vec(),
            0,
        )
        .unwrap_or_else(|e| panic!("{}", String::from_utf8(e).expect("Unable to form string")));

        assert_eq!(
            (initial_balance - balance_after_init) as u64,
            init_gas_spent
        );

        run_to_block(4, None);

        let handle_gas_spent = Gear::get_gas_spent(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            request,
            0,
        )
        .unwrap_or_else(|e| panic!("{}", String::from_utf8(e).expect("Unable to form string")));

        assert_eq!(
            balance_after_init - balance_after_handle,
            handle_gas_spent as u128
        );
    });
}

#[test]
fn gas_spent_precalculated() {
    let wat = r#"
	(module
		(import "env" "memory" (memory 0))
		(export "handle" (func $handle))
        (func $add (; 0 ;) (param $0 i32) (param $1 i32)
            (local $2 i32)
            get_local $0
            get_local $1
            i32.add
            set_local $2
        )
        (func $handle
            (call $add
                (i32.const 2)
                (i32.const 2)
            )
		)
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = submit_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("submit result was asserted");

        run_to_block(2, None);

        let gas_spent_1 = Gear::get_gas_spent(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
        )
        .unwrap_or_else(|e| panic!("{}", String::from_utf8(e).expect("Unable to form string")));

        let schedule = <Test as Config>::Schedule::get();

        let const_i64_cost = schedule.instruction_weights.i64const;
        let call_cost = schedule.instruction_weights.call;
        let set_local_cost = schedule.instruction_weights.local_set;
        let get_local_cost = schedule.instruction_weights.local_get;
        let add_cost = schedule.instruction_weights.i64add;
        let gas_cost = schedule.host_fn_weights.gas as u32; // gas call in handle and "add" func

        let total_cost = call_cost
            + const_i64_cost * 2
            + set_local_cost
            + get_local_cost * 2
            + add_cost
            + gas_cost * 2;

        assert_eq!(gas_spent_1, total_cost as u64);

        let (gas_spent_2, _) =
            calc_handle_gas_spent(USER_1.into_origin(), prog_id, EMPTY_PAYLOAD.to_vec());

        assert_eq!(gas_spent_1, gas_spent_2);
    });
}

mod utils {
    use frame_support::dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo};
    use gear_core::ids::{CodeId, MessageId, ProgramId};
    use sp_core::H256;
    use sp_std::convert::TryFrom;

    use super::{
        assert_ok, pallet, run_to_block, BalancesPallet, Event, GearPallet, MessageInfo, MockEvent,
        Origin, SystemPallet, Test,
    };
    use common::Origin as _;

    pub(super) const DEFAULT_GAS_LIMIT: u64 = 5_000;
    pub(super) const DEFAULT_SALT: &[u8; 4] = b"salt";
    pub(super) const EMPTY_PAYLOAD: &[u8; 0] = b"";

    pub(super) type DispatchCustomResult<T> = Result<T, DispatchErrorWithPostInfo>;
    pub(super) type AccountId = <Test as frame_system::Config>::AccountId;
    pub(super) type GasPrice = <Test as pallet::Config>::GasPrice;
    type BlockNumber = <Test as frame_system::Config>::BlockNumber;

    pub(super) fn init_logger() {
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    pub(super) fn check_init_success(expected: u32) {
        let mut actual_children_amount = 0;
        SystemPallet::<Test>::events().iter().for_each(|e| {
            if let MockEvent::Gear(Event::InitSuccess(_)) = e.event {
                actual_children_amount += 1
            }
        });

        assert_eq!(expected, actual_children_amount);
    }

    pub(super) fn check_dequeued(expected: u32) {
        let mut actual_dequeued = 0;
        SystemPallet::<Test>::events().iter().for_each(|e| {
            if let MockEvent::Gear(Event::MessagesDequeued(num)) = e.event {
                actual_dequeued += num
            }
        });

        assert_eq!(expected, actual_dequeued);
    }

    pub(super) fn check_dispatched(expected: u32) {
        let mut actual_dispatched = 0;
        SystemPallet::<Test>::events().iter().for_each(|e| {
            if let MockEvent::Gear(Event::MessageDispatched(_)) = e.event {
                actual_dispatched += 1
            }
        });

        assert_eq!(expected, actual_dispatched);
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

        increase_prog_balance_for_mailbox_test(user, prog_id);
        populate_mailbox_from_program(prog_id, user, 2, 2_000_000_000, 0)
    }

    // Puts message from `prog_id` for the `user` in mailbox and returns its id
    pub(super) fn populate_mailbox_from_program(
        prog_id: H256,
        sender: AccountId,
        block_num: BlockNumber,
        gas_limit: u64,
        value: u128,
    ) -> H256 {
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(sender),
            prog_id,
            Vec::new(),
            gas_limit, // `prog_id` program sends message in handle which sets gas limit to 10_000_000.
            value,
        ));

        let message_id = get_last_message_id();
        run_to_block(block_num, None);

        {
            let expected_code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
            assert_eq!(
                common::get_program(prog_id)
                    .and_then(|p| common::ActiveProgram::try_from(p).ok())
                    .expect("program must exist")
                    .code_hash,
                generate_code_hash(&expected_code).into(),
                "can invoke send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
            );
        }

        MessageId::generate_outgoing(MessageId::from_origin(message_id), 0).into_origin()
    }

    pub(super) fn increase_prog_balance_for_mailbox_test(sender: AccountId, program_id: H256) {
        let expected_code_hash: H256 = generate_code_hash(
            ProgramCodeKind::OutgoingWithValueInHandle
                .to_bytes()
                .as_slice(),
        )
        .into();
        let actual_code_hash = common::get_program(program_id)
            .and_then(|p| common::ActiveProgram::try_from(p).ok())
            .map(|prog| prog.code_hash)
            .expect("invalid program address for the test");
        assert_eq!(
            expected_code_hash, actual_code_hash,
            "invalid program code for the test"
        );

        // This value is actually a constants in `ProgramCodeKind::OutgoingWithValueInHandle` wat. Alternatively can be read from Mailbox.
        let locked_value = 1000;

        // When program sends message, message value (if not 0) is reserved.
        // If value can't be reserved, message is skipped.
        assert_ok!(
            <BalancesPallet::<Test> as frame_support::traits::Currency<_>>::transfer(
                &sender,
                &AccountId::from_origin(program_id),
                locked_value,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );
    }

    // Submits program with default options (salt, gas limit, value, payload)
    pub(super) fn submit_program_default(
        user: AccountId,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<H256> {
        let code = code_kind.to_bytes();
        let salt = DEFAULT_SALT.to_vec();

        GearPallet::<Test>::submit_program(
            Origin::signed(user),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
        .map(|_| get_last_program_id())
    }

    pub(super) fn generate_program_id(code: &[u8], salt: &[u8]) -> H256 {
        ProgramId::generate(CodeId::generate(code), salt).into_origin()
    }

    pub(super) fn generate_code_hash(code: &[u8]) -> [u8; 32] {
        CodeId::generate(code).into()
    }

    pub(super) fn send_default_message(from: AccountId, to: H256) -> DispatchResultWithPostInfo {
        GearPallet::<Test>::send_message(
            Origin::signed(from),
            to,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
    }

    pub(super) fn get_last_program_id() -> H256 {
        let event = match SystemPallet::<Test>::events()
            .last()
            .map(|r| r.event.clone())
        {
            Some(MockEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        let MessageInfo { program_id, .. } = match event {
            Event::InitMessageEnqueued(info) => info,
            _ => unreachable!("expect Event::InitMessageEnqueued"),
        };

        program_id
    }

    pub(super) fn get_last_message_id() -> H256 {
        let event = match SystemPallet::<Test>::events()
            .last()
            .map(|r| r.event.clone())
        {
            Some(MockEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        match event {
            Event::InitMessageEnqueued(MessageInfo { message_id, .. }) => message_id,
            Event::Log(msg) => msg.id().into_origin(),
            Event::DispatchMessageEnqueued(MessageInfo { message_id, .. }) => message_id,
            _ => unreachable!("expect sending"),
        }
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
                        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32) (result i32)))
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
                            (if
                                (then unreachable)
                                (else)
                            )
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

    #[allow(unused)]
    pub(super) fn print_gear_events<T: crate::Config>() {
        let v = SystemPallet::<T>::events()
            .into_iter()
            .map(|r| r.event)
            .collect::<Vec<_>>();

        println!("Gear events");
        for (pos, line) in v.iter().enumerate() {
            println!("{}). {:?}", pos, line);
        }
    }
}
