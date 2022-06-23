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

use crate::{
    manager::HandleKind,
    mock::{
        new_test_ext, run_to_block, run_to_next_block, Event as MockEvent, Gear, GearProgram,
        Origin, System, Test, BLOCK_AUTHOR, LOW_BALANCE_USER, USER_1, USER_2, USER_3,
    },
    pallet, Config, Error, Event, GasInfo, GearProgramPallet, MailboxOf, Pallet as GearPallet,
    WaitlistOf, GasHandlerOf,
};
use codec::{Decode, Encode};
use common::{event::*, storage::*, CodeStorage, GasPrice as _, Origin as _, ValueTree};
use demo_compose::WASM_BINARY as COMPOSE_WASM_BINARY;
use demo_distributor::{Request, WASM_BINARY};
use demo_mul_by_const::WASM_BINARY as MUL_CONST_WASM_BINARY;
use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};
use demo_waiting_proxy::WASM_BINARY as WAITING_PROXY_WASM_BINARY;
use frame_support::{
    assert_noop, assert_ok, dispatch::Dispatchable, sp_runtime::traits::Zero, traits::Currency,
};
use frame_system::Pallet as SystemPallet;
use gear_core::{
    code::Code,
    ids::{CodeId, MessageId, ProgramId},
};
use gear_core_errors::*;
use pallet_balances::{self, Pallet as BalancesPallet};
use utils::*;

#[test]
fn unstoppable_block_execution_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let user_balance = BalancesPallet::<Test>::free_balance(USER_1) as u64;
        let executions_amount = 10;
        let balance_for_each_execution = user_balance / executions_amount;

        assert!(balance_for_each_execution < <Test as pallet_gas::Config>::BlockGasLimit::get());

        let program_id = {
            let res = submit_program_default(USER_2, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, None);

        let GasInfo {
            burned: expected_burned_gas,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

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

        assert_last_dequeued(executions_amount as u32);

        assert_eq!(pallet_gas::Pallet::<Test>::gas_allowance(), 0);

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
        let block_gas_limit = <Test as pallet_gas::Config>::BlockGasLimit::get();
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
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

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
            USER_2.into(),
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
        let actual_gas_burned = remaining_weight - pallet_gas::Pallet::<Test>::gas_allowance();
        assert_eq!(actual_gas_burned, 0);

        // Ensure all created imbalances along the way cancel each other
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
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

        let send_message_call = crate::mock::Call::Gear(crate::Call::<Test>::send_message {
            destination: program_id,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: DEFAULT_GAS_LIMIT,
            value: 0,
        });
        assert_noop!(
            send_message_call.dispatch(Origin::signed(LOW_BALANCE_USER)),
            Error::<Test>::NotEnoughBalanceForReserve
        );

        // Because destination is user, no gas will be reserved
        MailboxOf::<Test>::clear();
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(LOW_BALANCE_USER),
            USER_1.into(),
            EMPTY_PAYLOAD.to_vec(),
            1000,
            1000
        ));
        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        // Gas limit too high
        let block_gas_limit = <Test as pallet_gas::Config>::BlockGasLimit::get();
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

        assert_last_dequeued(2);

        assert_ok!(send_default_message(USER_1, USER_2.into()));
        assert_ok!(send_default_message(USER_1, program_id));

        run_to_block(3, None);

        // "Mail" from user to user should not be processed as messages
        assert_last_dequeued(1);
    });
}

#[test]
fn spent_gas_to_reward_block_author_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let block_author_initial_balance = BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);
        assert_ok!(submit_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(2, None);

        assert_last_dequeued(1);

        // The block author should be paid the amount of Currency equal to
        // the `gas_charge` incurred while processing the `InitProgram` message
        let gas_spent = GasPrice::gas_price(
            <Test as pallet_gas::Config>::BlockGasLimit::get()
                - pallet_gas::Pallet::<Test>::gas_allowance(),
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
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

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
            <Test as pallet_gas::Config>::BlockGasLimit::get()
                - pallet_gas::Pallet::<Test>::gas_allowance(),
        );
        assert!(user1_potential_msgs_spends > user1_actual_msgs_spends);
        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends
        );

        // All created gas cancels out
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    })
}

#[test]
fn restrict_start_section() {
    // This test checks, that code with start section cannot be handled in process queue.
    let wat = r#"
	(module
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(start $start)
		(func $init)
        (func $handle)
        (func $start
            unreachable
        )
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();
        let salt = DEFAULT_SALT.to_vec();
        GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            5_000_000,
            0,
        )
        .expect_err("Must throw err, because code contains start section");
    });
}

#[test]
fn memory_access_cases() {
    // This test access different pages in wasm linear memory.
    // Some pages accessed many times and some pages are freed and then allocated again
    // during one execution. This actions are helpful to identify problems with pages reallocations
    // and how lazy pages works with them.
    let wat = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "alloc" (func $alloc (param i32) (result i32)))
    (import "env" "free" (func $free (param i32)))
    (export "handle" (func $handle))
    (export "init" (func $init))
    (func $init
        ;; allocate 3 pages in init, so mem will contain 4 pages: 0, 1, 2, 3
        (block
            i32.const 0x0
            i32.const 0x3
            call $alloc
            i32.const 0x1
            i32.eq
            br_if 0
            unreachable
        )
        ;; free page 2, so pages 0, 1, 3 is allocated now
        (block
            i32.const 0x2
            call $free
        )
        ;; access page 1 and change it, so it will have data in storage
        (block
            i32.const 0x10001
            i32.const 0x42
            i32.store
        )
    )
    (func $handle
        (block
            i32.const 0x0
            i32.load
            i32.eqz
            br_if 0

            ;; second run check that pages are in correct state

            ;; 1st page
            (block
                i32.const 0x10001
                i32.load
                i32.const 0x142
                i32.eq
                br_if 0
                unreachable
            )

            ;; 2nd page
            (block
                i32.const 0x20001
                i32.load
                i32.const 0x42
                i32.eq
                br_if 0
                unreachable
            )

            ;; 3th page
            (block
                i32.const 0x30001
                i32.load
                i32.const 0x42
                i32.eq
                br_if 0
                unreachable
            )

            br 1
        )

        ;; in first run access pages

        ;; alloc 2nd page
        (block
            i32.const 1
            call $alloc
            i32.const 2
            i32.eq
            br_if 0
            unreachable
        )
        ;; We freed 2nd page in init, so data will be default
        (block
            i32.const 0x20001
            i32.load
            i32.eqz
            br_if 0
            unreachable
        )
        ;; change 2nd page data
        i32.const 0x20001
        i32.const 0x42
        i32.store
        ;; free 2nd page
        i32.const 2
        call $free
        ;; alloc it again
        (block
            i32.const 1
            call $alloc
            i32.const 2
            i32.eq
            br_if 0
            unreachable
        )
        ;; write the same value
        i32.const 0x20001
        i32.const 0x42
        i32.store

        ;; 3th page. We have not access it yet, so data will be default
        (block
            i32.const 0x30001
            i32.load
            i32.eqz
            br_if 0
            unreachable
        )
        ;; change 3th page data
        i32.const 0x30001
        i32.const 0x42
        i32.store
        ;; free 3th page
        i32.const 3
        call $free
        ;; then alloc it again
        (block
            i32.const 1
            call $alloc
            i32.const 3
            i32.eq
            br_if 0
            unreachable
        )
        ;; write the same value
        i32.const 0x30001
        i32.const 0x42
        i32.store

        ;; 1st page. We have accessed this page before
        (block
            i32.const 0x10001
            i32.load
            i32.const 0x42
            i32.eq
            br_if 0
            unreachable
        )
        ;; change 1st page data
        i32.const 0x10001
        i32.const 0x142
        i32.store
        ;; free 1st page
        i32.const 1
        call $free
        ;; then alloc it again
        (block
            i32.const 1
            call $alloc
            i32.const 1
            i32.eq
            br_if 0
            unreachable
        )
        ;; write the same value
        i32.const 0x10001
        i32.const 0x142
        i32.store

        ;; set new handle case
        i32.const 0x0
        i32.const 0x1
        i32.store
    )
)
"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();
        let salt = DEFAULT_SALT.to_vec();
        let prog_id = generate_program_id(&code, &salt);
        let res = GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            500_000_000,
            0,
        )
        .map(|_| prog_id);
        let pid = res.expect("submit result is not ok");

        run_to_block(2, Some(1_000_000_000));
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // First handle: access pages
        let res = GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            0,
        );
        assert_ok!(res);

        run_to_block(3, Some(1_000_000_000));
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // Second handle: check pages data
        let res = GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            0,
        );
        assert_ok!(res);

        run_to_block(4, Some(1_000_000_000));
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
    });
}

#[cfg(feature = "lazy-pages")]
#[test]
fn lazy_pages() {
    use gear_core::memory::{PageNumber, WasmPageNumber};
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
            ;; here we access two native pages, if native page is less or equal to 16kiB
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
                500_000_000,
                0,
            )
            .map(|_| prog_id);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, Some(1_000_000_000));
        log::debug!("submit done {:?}", pid);
        assert_last_dequeued(1);

        let res = GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            1000,
        );
        log::debug!("res = {:?}", res);
        assert_ok!(res);

        run_to_block(3, Some(1_000_000_000));

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
            all_wasm_pages
                .iter()
                .flat_map(|p| p.to_gear_pages_iter())
                .collect()
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

        // accessed from 2 wasm page, can be several gear and native pages:
        let first_page = (0x23ffe / PageNumber::size()) as u32;
        let second_page = (0x24001 / PageNumber::size()) as u32;
        expected_accessed.extend(page_to_accessed(first_page));
        expected_accessed.extend(page_to_accessed(second_page));

        // accessed from 5 wasm page:
        expected_accessed.extend(page_to_accessed((0x50000 / PageNumber::size()) as u32));

        // accessed from 8 and 9 wasm pages, must be several gear pages:
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

// TODO: ignored until #642 implemented
#[test]
#[ignore]
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
        let remaining_weight = 791822425 + 6228060 - 1; // calc gas pid1 + pid2 - 1

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
        assert_last_dequeued(2);

        // Count gas needed to process programs with default payload
        let GasInfo {
            burned: expected_gas_msg_to_pid1,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid1),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");
        let GasInfo {
            burned: expected_gas_msg_to_pid2,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid2),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        // TrapInHandle code kind is used because processing default payload in its
        // context requires such an amount of gas, that the following assertion can be passed.
        assert!(expected_gas_msg_to_pid1 + expected_gas_msg_to_pid2 > remaining_weight);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            1000
        ));
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            1000
        ));

        run_to_block(3, Some(remaining_weight));
        assert_last_dequeued(2);

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
            2000
        ));
        let msg1 = get_last_message_id();

        let msg2_gas = remaining_weight;
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            msg2_gas,
            1000
        ));
        let _msg2 = get_last_message_id();

        let msg3_gas = expected_gas_msg_to_pid1;
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            expected_gas_msg_to_pid1,
            2000
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

        assert_failed(msg1, ExtError::Message(MessageError::NotEnoughGas));
        assert_last_dequeued(1);

        // Equals 0 due to trying execution of msg2.
        assert_eq!(pallet_gas::Pallet::<Test>::gas_allowance(), 0);

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

        assert_last_dequeued(2);
        assert_eq!(
            pallet_gas::Pallet::<Test>::gas_allowance(),
            last_block_allowance - real_gas_to_burn
        );
    });
}

#[test]
fn mailbox_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_1);

        // Ensure that all the gas has been returned to the sender upon messages processing
        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

        let mailbox_message = {
            let res = MailboxOf::<Test>::remove(USER_1, reply_to_id);
            assert!(res.is_ok());
            res.expect("was asserted previously")
        };

        assert_eq!(mailbox_message.id(), reply_to_id);

        // Gas limit should have been ignored by the code that puts a message into a mailbox
        assert_eq!(mailbox_message.value(), 1000);

        // Gas is not passed to mailboxed messages and should have been all spent by now
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    })
}

#[test]
fn init_message_logging_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let mut next_block = 2;

        let codes = [
            (ProgramCodeKind::Default, None),
            // Will fail, because tests use default gas limit, which is very low for successful greedy init
            (
                ProgramCodeKind::GreedyInit,
                Some(ExtError::Execution(ExecutionError::GasLimitExceeded)),
            ),
        ];

        for (code_kind, trap) in codes {
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

            let msg_id = match event {
                Event::MessageEnqueued { id, entry, .. } => {
                    if entry == Entry::Init {
                        id
                    } else {
                        unreachable!("expect Event::InitMessageEnqueued")
                    }
                }
                _ => unreachable!("expect Event::InitMessageEnqueued"),
            };

            if let Some(trap) = trap {
                assert_failed(msg_id, trap);
            } else {
                assert_succeed(msg_id);
            }

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

        // TODO: replace this unknown errors (`ExtError::Some`) with real ones.
        let tests = [
            // Code, init failure reason, handle succeed flag
            (ProgramCodeKind::Default, None, None),
            (
                ProgramCodeKind::GreedyInit,
                Some(ExtError::Execution(ExecutionError::GasLimitExceeded)),
                Some(ExtError::Some),
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_init),
                Some(ExtError::Some),
                Some(ExtError::Some),
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_handle),
                None,
                Some(ExtError::Some),
            ),
        ];

        for (code_kind, init_failure_reason, handle_failure_reason) in tests {
            SystemPallet::<Test>::reset_events();
            let program_id = {
                let res = submit_program_default(USER_1, code_kind);
                assert_ok!(res);
                res.expect("submit result was asserted")
            };

            let message_id = get_last_message_id();

            SystemPallet::<Test>::assert_last_event(
                Event::MessageEnqueued {
                    id: message_id,
                    source: USER_1,
                    destination: program_id,
                    entry: Entry::Init,
                }
                .into(),
            );

            run_to_block(next_block, None);
            next_block += 1;

            // Init failed program checks
            if let Some(init_failure_reason) = init_failure_reason {
                assert_failed(message_id, init_failure_reason);

                // Sending messages to failed-to-init programs shouldn't be allowed
                assert_noop!(
                    send_default_message(USER_1, program_id),
                    Error::<Test>::ProgramIsTerminated
                );

                continue;
            }

            assert_succeed(message_id);

            // Messages to fully-initialized programs are accepted
            assert_ok!(send_default_message(USER_1, program_id));

            let message_id = get_last_message_id();

            SystemPallet::<Test>::assert_last_event(
                Event::MessageEnqueued {
                    id: message_id,
                    source: USER_1,
                    destination: program_id,
                    entry: Entry::Handle,
                }
                .into(),
            );

            run_to_block(next_block, None);

            if let Some(handle_failure_reason) = handle_failure_reason {
                assert_failed(message_id, handle_failure_reason);
            } else {
                assert_succeed(message_id);
            }

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
                &AccountId::from_origin(prog_id.into_origin()),
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
        let expected_reply_message_id = get_last_message_id();

        // global nonce is 2 before sending reply message
        // `submit_program` and `send_message` messages were sent before in `setup_mailbox_test_state`
        let event = match SystemPallet::<Test>::events()
            .last()
            .map(|r| r.event.clone())
        {
            Some(MockEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        let actual_reply_message_id = match event {
            Event::MessageEnqueued {
                id,
                entry: Entry::Reply(_reply_to_id),
                ..
            } => id,
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
                MessageId::from_origin(5.into_origin()), // non existent `reply_to_id`
                EMPTY_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                0
            ),
            pallet_gear_messenger::Error::<Test>::MailboxElementNotFound
        );

        let prog_id = {
            let res = submit_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        if let common::Program::Terminated =
            common::get_program(prog_id.into_origin()).expect("Failed to get program from storage")
        {
            panic!("Program is terminated!");
        };

        populate_mailbox_from_program(prog_id, USER_1, 2, 2_000_000_000, 0);

        // Program didn't have enough balance, so it's message produces trap
        // (and following system reply with error to USER_1 mailbox)
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);
        assert!(matches!(
            MailboxOf::<Test>::iter_key(USER_1)
                .next()
                .expect("Element should be")
                .reply(),
            Some((_, 1))
        ));
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
                &AccountId::from_origin(prog_id.into_origin()),
                send_to_program_amount,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );

        let mut next_block = 2;

        let user_messages_data = [
            // gas limit, value
            (1_000_000, 1000),
            (20_000_000, 2000),
        ];
        for (gas_limit_to_reply, value_to_reply) in user_messages_data {
            let reply_to_id =
                populate_mailbox_from_program(prog_id, USER_1, next_block, 2_000_000_000, 0);

            next_block += 1;

            assert!(!MailboxOf::<Test>::is_empty(&USER_1));

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

// TODO: ignored until #642 implemented
#[test]
#[ignore]
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
        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        let GasInfo {
            burned: gas_burned, ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");
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

        SystemPallet::<Test>::assert_last_event(
            Event::UserMessageRead {
                id: reply_to_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            }
            .into(),
        );
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
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_id = generate_program_id(WASM_BINARY, DEFAULT_SALT);

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            2_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            program_id,
            Request::Receive(10).encode(),
            200_000_000,
            0,
        ));

        run_to_block(3, None);

        // Despite some messages are still in the mailbox all gas locked in value trees
        // has been refunded to the sender so the free balances should add up
        let final_balance = BalancesPallet::<Test>::free_balance(USER_1)
            + BalancesPallet::<Test>::free_balance(BLOCK_AUTHOR);

        assert_eq!(initial_balance, final_balance);

        // All gas cancelled out in the end
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
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

        // TODO: replace this temporary (`None`) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        SystemPallet::<Test>::assert_last_event(
            Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            }
            .into(),
        );
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
        let code_id = generate_code_hash(&code).into();

        // First submit program, which will set code and metadata
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));

        // TODO: replace this temporary (`None`) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        SystemPallet::<Test>::assert_has_event(
            Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            }
            .into(),
        );
        assert!(<Test as Config>::CodeStorage::exists(code_id));

        // Trying to set the same code twice.
        assert_noop!(
            GearPallet::<Test>::submit_code(Origin::signed(USER_2), code),
            Error::<Test>::CodeAlreadyExists,
        );
    })
}

#[test]
fn test_code_is_not_reset_within_program_submission() {
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
            .filter(|e| {
                matches!(
                    e.event,
                    MockEvent::Gear(Event::CodeChanged {
                        change: CodeChangeKind::Active { .. },
                        ..
                    })
                )
            })
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
        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);

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

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();

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

        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);
        assert_eq!(
            MailboxOf::<Test>::iter_key(USER_1)
                .next()
                .expect("Element should be")
                .payload()
                .to_vec(),
            b"Hello, world!".encode()
        );
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

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            20_000_000_000u64,
            0,
        ));

        run_to_block(20, None);

        let actual_n = MailboxOf::<Test>::iter_key(USER_3).fold(0usize, |i, m| {
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
            1000
        ));
        let skipped_message_id = get_last_message_id();
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        run_to_block(2, None);
        // system reply message
        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        let user_balance_after = BalancesPallet::<Test>::free_balance(USER_1);
        assert_eq!(user_balance_before, user_balance_after);

        assert_not_executed(skipped_message_id);

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
            [0].to_vec(),
            50_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_terminated(program_id));
        assert!(!Gear::is_initialized(program_id));
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // Program is not removed and can't be submitted again
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                code,
                vec![],
                Vec::new(),
                2_000_000_000,
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
        // 2 error replies are generated, and executed (forwarded to USER_2 mailbox).
        assert_eq!(MailboxOf::<Test>::len(&USER_2), 2);
        assert_total_dequeued(4 + 2); // +2 for submit_program/send_messages
        assert_init_success(1); // 1 for submitting factory

        SystemPallet::<Test>::reset_events();
        MailboxOf::<Test>::clear();

        // Try to create multiple programs with non existing code hash
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (valid_code_hash, b"salt1".to_vec(), 2_000_000_000),
                (valid_code_hash, b"salt2".to_vec(), 2_000_000_000),
                (valid_code_hash, b"salt3".to_vec(), 2_000_000_000),
            ])
            .encode(),
            20_000_000_000,
            0,
        ));
        run_to_block(3, None);

        assert_eq!(MailboxOf::<Test>::len(&USER_2), 6);
        assert_total_dequeued(12 + 1);
        assert_init_success(0);

        assert_noop!(
            GearPallet::<Test>::submit_code(
                Origin::signed(USER_1),
                invalid_prog_code_kind.to_bytes(),
            ),
            Error::<Test>::FailedToConstructProgram,
        );

        SystemPallet::<Test>::reset_events();
        MailboxOf::<Test>::clear();

        // Try to create with invalid code hash
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (invalid_prog_code_hash, b"salt1".to_vec(), 2_000_000_000),
                (invalid_prog_code_hash, b"salt2".to_vec(), 2_000_000_000),
                (invalid_prog_code_hash, b"salt3".to_vec(), 2_000_000_000),
            ])
            .encode(),
            20_000_000_000,
            0,
        ));

        run_to_block(4, None);

        assert_eq!(MailboxOf::<Test>::len(&USER_2), 6);
        assert_total_dequeued(12 + 1);
        assert_init_success(0);
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
                vec![(child_code_hash, b"some_data".to_vec(), 300_000)] // too little gas
            )
            .encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // First extrinsic call with successful program creation dequeues and executes init and dispatch messages
        // Second extrinsic is failing one, for each message it generates replies, which are executed (4 dequeued, 2 dispatched)
        assert_total_dequeued(6 + 3); // +3 for extrinsics
        assert_init_success(1 + 1); // +1 for submitting factory

        SystemPallet::<Test>::reset_events();

        // Create multiple successful init programs
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt1".to_vec(), 1_000_000),
                (child_code_hash, b"salt2".to_vec(), 1_000_000),
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
                (child_code_hash, b"salt3".to_vec(), 300_000), // too little gas
                (child_code_hash, b"salt4".to_vec(), 300_000), // too little gas
            ])
            .encode(),
            4_000_000_000,
            0,
        ));
        run_to_block(6, None);

        assert_total_dequeued(12 + 2); // +2 for extrinsics
        assert_init_success(2);
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
            20_000_000_000,
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
            CreateProgram::Custom(vec![(
                child_code_hash,
                DEFAULT_SALT.to_vec(),
                2_000_000_000
            )])
            .encode(),
            20_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // When duplicate try happens, init is not executed, a reply is generated and executed (+2 dequeued, +1 dispatched)
        // Concerning dispatch message, it is executed, because destination exists (+1 dispatched, +1 dequeued)
        assert_eq!(MailboxOf::<Test>::len(&USER_2), 1);
        assert_total_dequeued(3 + 3); // +3 from extrinsics (2 submit_program, 1 send_message)
        assert_init_success(2); // +2 from extrinsics (2 submit_program)

        SystemPallet::<Test>::reset_events();
        MailboxOf::<Test>::clear();

        // Create a new program from program
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 2_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
        ));
        run_to_block(5, None);

        // Try to create the same
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 2_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
        ));
        run_to_block(6, None);

        // First call successfully creates a program and sends a messages to it (+2 dequeued, +1 dispatched)
        // Second call will not cause init message execution, but a reply will be generated (+2 dequeued, +1 dispatched)
        // Handle message from the second call will be executed (addressed for existing destination) (+1 dequeued, +1 dispatched)
        assert_eq!(MailboxOf::<Test>::len(&USER_2), 1);
        assert_total_dequeued(5 + 2); // +2 from extrinsics (send_message)
        assert_init_success(1);

        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                child_code,
                b"salt1".to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                10_000_000_000,
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
                (child_code_hash, b"salt1".to_vec(), 1_000_000_000), // could be successful init
                (child_code_hash, b"salt1".to_vec(), 1_000_000_000), // duplicate
            ])
            .encode(),
            20_000_000_000,
            0,
        ));

        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        run_to_block(3, None);

        // Duplicate init fails the call and returns error reply to the caller, which is USER_1.
        // State roll-back is performed.
        assert_total_dequeued(2); // 2 for extrinsics
        assert_init_success(1); // 1 for creating a factory

        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        SystemPallet::<Test>::reset_events();
        MailboxOf::<Test>::clear();

        // Successful child creation
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 1_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
        ));

        run_to_block(4, None);

        assert!(MailboxOf::<Test>::is_empty(&USER_2));
        assert_total_dequeued(2 + 1); // 1 for extrinsics
        assert_init_success(1);
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
                (child1_code_hash, b"salt1".to_vec(), 1_000_000),
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child1_code_hash, b"salt2".to_vec(), 100_000),
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
                (child2_code_hash, b"salt1".to_vec(), 300_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt2".to_vec(), 1_000_000),
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
                (child2_code_hash, b"salt1".to_vec(), 1_000_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt3".to_vec(), 1_000_000),
            ])
            .encode(),
            5_000_000_000,
            0,
        ));

        run_to_block(5, None);

        assert_total_dequeued(18 + 4); // +4 for 3 send_message calls and 1 submit_program call
        assert_init_success(3 + 1); // +1 for submitting factory
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
        assert!(MailboxOf::<Test>::is_empty(&USER_3));
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
                2_000_000_000,
                0u128
            ),
            Error::<Test>::ProgramAlreadyExists,
        );
    })
}

#[test]
fn no_redundant_gas_value_after_exiting() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_exit_handle::WASM_BINARY;

        let prog_id = generate_program_id(WASM_BINARY, DEFAULT_SALT);
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            0,
        ));

        run_to_block(2, None);

        let GasInfo {
            min_limit: gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");
        assert_ok!(GearPallet::<Test>::send_message(
            Origin::signed(USER_1),
            prog_id,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent,
            0,
        ));

        let msg_id = get_last_message_id().into_origin();
        let maybe_limit = GasHandlerOf::<Test>::get_limit(msg_id).expect("invalid algo");
        assert_eq!(maybe_limit, Some(gas_spent));

        // before execution
        let free_after_send = BalancesPallet::<Test>::free_balance(USER_1);
        let reserved_after_send = BalancesPallet::<Test>::reserved_balance(USER_1);
        assert_eq!(reserved_after_send, gas_spent as u128);

        run_to_block(3, None);

        // gas_limit has been recovered
        let maybe_limit = GasHandlerOf::<Test>::get_limit(msg_id).expect("invalid algo");
        assert_eq!(maybe_limit, None);

        // the (reserved_after_send - gas_spent) has been unreserved
        let free_after_execution = BalancesPallet::<Test>::free_balance(USER_1);
        assert_eq!(
            free_after_execution,
            free_after_send + (reserved_after_send - gas_spent as u128)
        );

        // reserved balance after execution is zero
        let reserved_after_execution = BalancesPallet::<Test>::reserved_balance(USER_1);
        assert!(reserved_after_execution.is_zero());
    })
}

#[test]
fn init_wait_reply_exit_cleaned_storage() {
    use demo_init_wait_reply_exit::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            Vec::new(),
            2_000_000_000u64,
            0u128
        ));
        let pid = get_last_program_id();

        // block 2
        //
        // - send messages to the program
        run_to_block(2, None);
        let count = 5;
        for _ in 0..count {
            assert_ok!(GearPallet::<Test>::send_message(
                Origin::signed(USER_1),
                pid,
                vec![],
                10_000u64,
                0u128
            ));
        }

        // block 3
        //
        // - count waiting init messages
        // - reply and wake program
        // - check program status
        run_to_block(3, None);
        assert_eq!(waiting_init_messages(pid).len(), count);
        assert_eq!(WaitlistOf::<Test>::iter_key(pid).count(), count + 1);

        let msg_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();
        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            msg_id,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000_000u64,
            0,
        ));

        assert!(!Gear::is_initialized(pid));
        assert!(!Gear::is_terminated(pid));

        // block 4
        //
        // - check if program has terminated
        // - check waiting_init storage is empty
        // - check wait list is empty
        run_to_block(4, None);
        assert!(!Gear::is_initialized(pid));
        assert!(Gear::is_terminated(pid));
        assert_eq!(waiting_init_messages(pid).len(), 0);
        assert_eq!(WaitlistOf::<Test>::iter_key(pid).count(), 0);
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

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();

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
                destination: paused_program_id.into_origin().into()
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
                &<utils::AccountId as common::Origin>::from_origin(program_id.into_origin())
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

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .expect("Element should be")
            .id();

        assert_ok!(GearPallet::<Test>::send_reply(
            Origin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            2_000_000_000u64,
            1_000u128,
        ));

        run_to_block(3, None);

        let program = match common::get_program(program_id.into_origin()).expect("program exists") {
            common::Program::Active(p) => p,
            _ => unreachable!(),
        };

        let memory_pages = common::get_program_pages_data(program_id.into_origin(), &program)
            .unwrap()
            .into_iter()
            .map(|(page, data)| (page, data.into_vec()))
            .collect();

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

        let actual_n = MailboxOf::<Test>::iter_key(USER_3).fold(0usize, |i, m| {
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
            1_000_000_000,
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
        let total_balance_after_handle = BalancesPallet::<Test>::total_balance(&USER_1);

        let GasInfo {
            min_limit: init_gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(WASM_BINARY.to_vec()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap_or_else(|e| panic!("{}", String::from_utf8(e).expect("Unable to form string")));

        // check that all changes made by calculate_gas_info are rollbacked
        assert_eq!(
            balance_after_handle,
            BalancesPallet::<Test>::free_balance(USER_1)
        );
        assert_eq!(
            total_balance_after_handle,
            BalancesPallet::<Test>::total_balance(&USER_1)
        );

        assert_eq!(
            (initial_balance - balance_after_init) as u64,
            init_gas_spent
        );

        run_to_block(4, None);

        let GasInfo {
            min_limit: handle_gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            request,
            0,
            true,
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

        let GasInfo {
            min_limit: gas_spent_1,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
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

        let GasInfo {
            min_limit: gas_spent_2,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_eq!(gas_spent_1, gas_spent_2);
    });
}

#[test]
fn test_two_contracts_composition_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let contract_a_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract_a");
        let contract_b_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract_b");
        let compose_id = generate_program_id(COMPOSE_WASM_BINARY, b"salt");

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"contract_a".to_vec(),
            50_u64.encode(),
            400_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"contract_b".to_vec(),
            75_u64.encode(),
            400_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            COMPOSE_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            (
                <[u8; 32]>::from(contract_a_id),
                <[u8; 32]>::from(contract_b_id)
            )
                .encode(),
            400_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            Origin::signed(USER_1),
            compose_id,
            100_u64.to_le_bytes().to_vec(),
            10_000_000_000,
            0,
        ));

        run_to_block(4, None);

        // Gas total issuance should have gone back to 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    });
}

// Before introducing this test, submit_program extrinsic didn't check the value.
// Also value wasn't check in `create_program` sys-call. There could be the next test case, which could affect badly.
//
// User submits program with value X, which is not checked. Say X < ED. If we send handle and reply messages with
// values during the init message processing, internal checks will result in errors (either, because sending value
// Y <= X < ED is not allowed, or because of Y > X, when X < ED).
// However, in this same situation of program being initialized and sending some message with value, if program send
// init message with value Y <= X < ED, no internal checks will occur, so such message sending will be passed further
// to manager, although having value less than ED.
//
// Note: on manager level message will not be included to the [queue](https://github.com/gear-tech/gear/blob/master/pallets/gear/src/manager.rs#L351-L364)
// But it's is not preferable to enter that `if` clause.
#[test]
fn test_create_program_with_value_lt_ed() {
    use demo_init_with_value::{SendMessage, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Ids of custom destinations
        let ed = get_ed();
        let msg_receiver_1 = 5u64;
        let msg_receiver_2 = 6u64;

        // Submit the code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        // Can't initialize program with value less than ED
        assert_noop!(
            GearPallet::<Test>::submit_program(
                Origin::signed(USER_1),
                ProgramCodeKind::Default.to_bytes(),
                b"test0".to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                10_000_000,
                ed - 1,
            ),
            Error::<Test>::ValueLessThanMinimal,
        );

        // Simple passing test with values
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"test1".to_vec(),
            // Sending 500 value with "handle" messages. This should not fail.
            // Must be stated, that "handle" messages send value to some non-existing address
            // so messages will go to mailbox
            vec![
                SendMessage::Handle {
                    destination: msg_receiver_1,
                    value: 500
                },
                SendMessage::Handle {
                    destination: msg_receiver_2,
                    value: 500
                },
                SendMessage::Init { value: 0 },
            ]
            .encode(),
            10_000_000_000,
            1000,
        ));

        run_to_block(2, None);

        // init messages sent by user and by program
        assert_total_dequeued(2);
        // programs deployed by user and by program
        assert_init_success(2);

        let origin_msg_id =
            MessageId::generate_from_user(1, ProgramId::from_origin(USER_1.into_origin()), 0);
        let msg1_mailbox = MessageId::generate_outgoing(origin_msg_id, 0);
        let msg2_mailbox = MessageId::generate_outgoing(origin_msg_id, 1);
        assert!(MailboxOf::<Test>::contains(&msg_receiver_1, &msg1_mailbox));
        assert!(MailboxOf::<Test>::contains(&msg_receiver_2, &msg2_mailbox));

        SystemPallet::<Test>::reset_events();

        // Trying to send init message from program with value less than ED.
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"test2".to_vec(),
            // First two messages won't fail, because provided values are in a valid range
            // The last message value (which is the value of init message) will end execution with trap
            vec![
                SendMessage::Handle {
                    destination: msg_receiver_1,
                    value: 500
                },
                SendMessage::Handle {
                    destination: msg_receiver_2,
                    value: 500
                },
                SendMessage::Init { value: ed - 1 },
            ]
            .encode(),
            10_000_000_000,
            1000,
        ));

        let msg_id = get_last_message_id();

        run_to_block(3, None);

        // User's message execution will result in trap, because program tries
        // to send init message with value in invalid range. As a result, 1 dispatch
        // is dequeued (user's  message) and one message is sent to mailbox.
        let mailbox_msg_id = get_last_message_id();
        assert!(MailboxOf::<Test>::contains(&USER_1, &mailbox_msg_id));

        // This check means, that program's invalid init message didn't reach the queue.
        assert_total_dequeued(1);

        assert_failed(
            msg_id,
            ExtError::Message(MessageError::InsufficientValue {
                message_value: 499,
                existential_deposit: 500,
            }),
        );
    })
}

// Before introducing this test, submit_program extrinsic didn't check the value.
// Also value wasn't check in `create_program` sys-call. There could be the next test case, which could affect badly.
//
// For instance, we have a guarantee that provided init message value is more than ED before executing message.
// User sends init message to the program, which, for example, in init function sends different kind of messages.
// Because of message value not being checked for init messages, program can send more value amount within init message,
// then it has on it's balance. Such message send will end up without any error/trap. So all in all execution will end
// up successfully with messages sent from program with total value more than was provided to the program.
//
// Again init message won't be added to the queue, because of the check here (https://github.com/gear-tech/gear/blob/master/pallets/gear/src/manager.rs#L351-L364).
// But it's is not preferable to enter that `if` clause.
#[test]
fn test_create_program_with_exceeding_value() {
    use demo_init_with_value::{SendMessage, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Submit the code
        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        let sending_to_program = 2 * get_ed();
        let random_receiver = 1;
        // Trying to send init message from program with value greater than program can send.
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"test1".to_vec(),
            vec![
                SendMessage::Handle {
                    destination: random_receiver,
                    value: sending_to_program / 3
                },
                SendMessage::Handle {
                    destination: random_receiver,
                    value: sending_to_program / 3
                },
                SendMessage::Init {
                    value: sending_to_program + 1,
                },
            ]
            .encode(),
            10_000_000_000,
            sending_to_program,
        ));

        run_to_block(2, None);

        // Check there are no messages for `random_receiver`. There would be messages in mailbox
        // if execution didn't end up with an "Not enough value to send message" error.
        let origin_msg_id =
            MessageId::generate_from_user(1, ProgramId::from_origin(USER_1.into_origin()), 0);
        let receiver_mail_msg1 = MessageId::generate_outgoing(origin_msg_id, 0);
        let receiver_mail_msg2 = MessageId::generate_outgoing(origin_msg_id, 1);
        assert!(!MailboxOf::<Test>::contains(
            &random_receiver,
            &receiver_mail_msg1
        ));
        assert!(!MailboxOf::<Test>::contains(
            &random_receiver,
            &receiver_mail_msg2
        ));

        // User's message execution will result in trap, because program tries
        // to send init message with value more than program has. As a result, 1 dispatch
        // is dequeued (user's  message) and one message is sent to mailbox.
        let mailbox_msg_id = get_last_message_id();
        assert!(MailboxOf::<Test>::contains(&USER_1, &mailbox_msg_id));

        // This check means, that program's invalid init message didn't reach the queue.
        assert_total_dequeued(1);

        assert_failed(
            origin_msg_id,
            ExtError::Message(MessageError::NotEnoughValue {
                message_value: 1001,
                value_left: 1000,
            }),
        );
    })
}

#[test]
fn test_create_program_without_gas_works() {
    use demo_init_with_value::{SendMessage, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(GearPallet::<Test>::submit_code(
            Origin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"test1".to_vec(),
            vec![SendMessage::InitWithoutGas { value: 0 }].encode(),
            10_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_total_dequeued(2);
        assert_init_success(2);
    })
}

#[test]
fn test_reply_to_terminated_program() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_exit_init::WASM_BINARY;

        // Deploy program, which sends mail and exits
        assert_ok!(GearPallet::<Test>::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            // this input makes it first send message to mailbox and then exit
            [1].to_vec(),
            27_100_000_000u64,
            0
        ));

        let mail_id = {
            let original_message_id = get_last_message_id();
            MessageId::generate_reply(original_message_id, 0)
        };

        run_to_block(2, None);

        // Check mail in Mailbox
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);

        // Send reply
        let reply_call = crate::mock::Call::Gear(crate::Call::<Test>::send_reply {
            reply_to_id: mail_id,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: 10_000_000,
            value: 0,
        });
        assert_noop!(
            reply_call.dispatch(Origin::signed(USER_1)),
            Error::<Test>::ProgramIsTerminated,
        );

        log::debug!("mailbox: {:?}", MailboxOf::<Test>::iter_key(USER_1).next());

        // the only way to claim value from terminated destination is a corresponding extrinsic call
        assert_ok!(GearPallet::<Test>::claim_value_from_mailbox(
            Origin::signed(USER_1),
            mail_id,
        ));

        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        SystemPallet::<Test>::assert_last_event(
            Event::UserMessageRead {
                id: mail_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            }
            .into(),
        )
    })
}

#[test]
fn cascading_messages_with_value_do_not_overcharge() {
    init_logger();
    new_test_ext().execute_with(|| {
        let contract_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract");
        let wrapper_id = generate_program_id(WAITING_PROXY_WASM_BINARY, b"salt");

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"contract".to_vec(),
            50_u64.encode(),
            400_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            WAITING_PROXY_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            <[u8; 32]>::from(contract_id).encode(),
            400_000_000,
            0,
        ));

        run_to_block(2, None);

        let payload = 100_u64.to_le_bytes().to_vec();

        let user_balance_before_calculating = BalancesPallet::<Test>::free_balance(USER_1);

        run_to_block(3, None);

        // The constant added for checks.
        let value = 10_000_000;

        let GasInfo {
            min_limit: gas_reserved,
            burned: gas_to_spend,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(wrapper_id),
            payload.clone(),
            value,
            true,
        )
        .expect("Failed to get gas spent");

        assert!(gas_reserved >= gas_to_spend);

        run_to_block(4, None);

        // A message is sent to a waiting proxy contract that passes execution
        // on to another contract while keeping the `value`.
        // The overall gas expenditure is `gas_to_spend`. The message gas limit
        // is set to be just enough to cover this amount.
        // The sender's account has enough funds for both gas and `value`,
        // therefore expecting the message to be processed successfully.
        // Expected outcome: the sender's balance has decreased by the
        // (`gas_to_spend` + `value`).

        let user_initial_balance = BalancesPallet::<Test>::free_balance(USER_1);

        assert_eq!(user_balance_before_calculating, user_initial_balance);
        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

        assert_ok!(Gear::send_message(
            Origin::signed(USER_1),
            wrapper_id,
            payload,
            gas_reserved,
            value,
        ));

        let gas_to_spend = GasPrice::gas_price(gas_to_spend);
        let gas_reserved = GasPrice::gas_price(gas_reserved);
        let reserved_balance = gas_reserved + value;

        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user_initial_balance - reserved_balance
        );

        assert_eq!(
            BalancesPallet::<Test>::reserved_balance(USER_1),
            reserved_balance
        );

        run_to_block(5, None);

        assert_eq!(BalancesPallet::<Test>::reserved_balance(USER_1), 0);

        assert_eq!(
            BalancesPallet::<Test>::free_balance(USER_1),
            user_initial_balance - gas_to_spend - value
        );
    });
}

#[test]
fn call_forbidden_function() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_gas_available" (func $gr_gas_available (result i64)))
        (export "handle" (func $handle))
        (func $handle
            call $gr_gas_available
            drop
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = submit_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("submit result was asserted");

        run_to_block(2, None);

        let res = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .map_err(|e| String::from_utf8(e).unwrap());

        assert_eq!(
            res,
            Err("Program terminated with a trap: Unable to call a forbidden function".to_string())
        );
    });
}

#[test]
fn test_async_messages() {
    use demo_async_tester::{Kind, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::submit_program(
            Origin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000u64,
            0,
        ));

        let pid = get_last_program_id();
        for kind in &[
            Kind::Reply,
            Kind::ReplyWithGas(DEFAULT_GAS_LIMIT),
            Kind::ReplyBytes,
            Kind::ReplyBytesWithGas(DEFAULT_GAS_LIMIT),
            Kind::ReplyCommit,
            Kind::ReplyCommitWithGas(DEFAULT_GAS_LIMIT),
            Kind::Send,
            Kind::SendWithGas(DEFAULT_GAS_LIMIT),
            Kind::SendBytes,
            Kind::SendBytesWithGas(DEFAULT_GAS_LIMIT),
            Kind::SendCommit,
            Kind::SendCommitWithGas(DEFAULT_GAS_LIMIT),
        ] {
            run_to_next_block(None);
            assert_ok!(Gear::send_message(
                Origin::signed(USER_1),
                pid,
                kind.encode(),
                3_000_000_000u64,
                0,
            ));

            // check the message sent from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(Kind::decode(&mut last_mail.payload()), Ok(*kind));

            // reply to the message
            let message_id = last_mail.id();
            assert_ok!(Gear::send_reply(
                Origin::signed(USER_1),
                message_id,
                EMPTY_PAYLOAD.to_vec(),
                2_000_000_000u64,
                0,
            ));

            // check the reply from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(last_mail.payload(), b"PONG");
            MailboxOf::<Test>::remove(USER_1, last_mail.id()).expect("remove last mail failed");
        }

        assert!(!Gear::is_terminated(pid));
    })
}

mod utils {
    #![allow(unused)]

    use super::{
        assert_ok, pallet, run_to_block, BalancesPallet, Event, GearPallet, MailboxOf, MockEvent,
        Origin, SystemPallet, Test,
    };
    use codec::Decode;
    use common::{event::*, storage::IterableByKeyMap, Origin as _};
    use frame_support::{
        dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
        traits::tokens::currency::Currency,
    };
    use gear_core::{
        ids::{CodeId, MessageId, ProgramId},
        message::StoredMessage,
    };
    use sp_core::H256;
    use sp_runtime::traits::UniqueSaturatedInto;
    use sp_std::{convert::TryFrom, fmt::Debug};

    pub(super) const DEFAULT_GAS_LIMIT: u64 = 500_000;
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

    pub(super) fn get_ed() -> u128 {
        <Test as pallet::Config>::Currency::minimum_balance().unique_saturated_into()
    }

    pub(super) fn assert_init_success(expected: u32) {
        let mut actual_children_amount = 0;
        SystemPallet::<Test>::events().iter().for_each(|e| {
            if let MockEvent::Gear(Event::ProgramChanged {
                change: ProgramChangeKind::Active { .. },
                ..
            }) = e.event
            {
                actual_children_amount += 1
            }
        });

        assert_eq!(expected, actual_children_amount);
    }

    pub(super) fn assert_last_dequeued(expected: u32) {
        let last_dequeued = SystemPallet::<Test>::events()
            .iter()
            .filter_map(|e| {
                if let MockEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                    Some(total)
                } else {
                    None
                }
            })
            .last()
            .expect("Not found Event::MessagesDispatched");

        assert_eq!(expected, last_dequeued);
    }

    pub(super) fn assert_total_dequeued(expected: u32) {
        let actual_dequeued: u32 = SystemPallet::<Test>::events()
            .iter()
            .filter_map(|e| {
                if let MockEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                    Some(total)
                } else {
                    None
                }
            })
            .sum();

        assert_eq!(expected, actual_dequeued);
    }

    // Creates a new program and puts message from program to `user` in mailbox
    // using extrinsic calls. Imitates real-world sequence of calls.
    //
    // *NOTE*:
    // 1) usually called inside first block
    // 2) runs to block 2 all the messages place to message queue/storage
    //
    // Returns id of the message in the mailbox
    pub(super) fn setup_mailbox_test_state(user: AccountId) -> MessageId {
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
        prog_id: ProgramId,
        sender: AccountId,
        block_num: BlockNumber,
        gas_limit: u64,
        value: u128,
    ) -> MessageId {
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
                common::get_program(prog_id.into_origin())
                    .and_then(|p| common::ActiveProgram::try_from(p).ok())
                    .expect("program must exist")
                    .code_hash,
                generate_code_hash(&expected_code).into(),
                "can invoke send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
            );
        }

        MessageId::generate_outgoing(message_id, 0)
    }

    pub(super) fn increase_prog_balance_for_mailbox_test(sender: AccountId, program_id: ProgramId) {
        let expected_code_hash: H256 = generate_code_hash(
            ProgramCodeKind::OutgoingWithValueInHandle
                .to_bytes()
                .as_slice(),
        )
        .into();
        let actual_code_hash = common::get_program(program_id.into_origin())
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
                &AccountId::from_origin(program_id.into_origin()),
                locked_value,
                frame_support::traits::ExistenceRequirement::AllowDeath
            )
        );
    }

    // Submits program with default options (salt, gas limit, value, payload)
    pub(super) fn submit_program_default(
        user: AccountId,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<ProgramId> {
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

    pub(super) fn generate_program_id(code: &[u8], salt: &[u8]) -> ProgramId {
        ProgramId::generate(CodeId::generate(code), salt)
    }

    pub(super) fn generate_code_hash(code: &[u8]) -> [u8; 32] {
        CodeId::generate(code).into()
    }

    pub(super) fn send_default_message(
        from: AccountId,
        to: ProgramId,
    ) -> DispatchResultWithPostInfo {
        GearPallet::<Test>::send_message(
            Origin::signed(from),
            to,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
    }

    pub(super) fn dispatch_status(message_id: MessageId) -> Option<DispatchStatus> {
        let mut found_status: Option<DispatchStatus> = None;
        SystemPallet::<Test>::events().iter().for_each(|e| {
            if let MockEvent::Gear(Event::MessagesDispatched { statuses, .. }) = &e.event {
                found_status = statuses.get(&message_id).map(Clone::clone);
            }
        });

        found_status
    }

    pub(super) fn assert_dispatched(message_id: MessageId) {
        assert!(dispatch_status(message_id).is_some())
    }

    pub(super) fn assert_succeed(message_id: MessageId) {
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::Success)
    }

    pub(super) fn assert_failed<D>(message_id: MessageId, _error: D)
    where
        D: Decode + Debug + PartialEq,
    {
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::Failed);

        // TODO: uncomment code below, once issue #970 resolved.

        // let mut actual_error = None;

        // SystemPallet::<Test>::events().iter().for_each(|e| {
        //     if let MockEvent::Gear(Event::UserMessageSent {
        //         message,
        //         ..
        //     }) = e.event
        //     {
        //         if let Some((id, exit_code)) = message.reply() {
        //             if id == message_id {
        //                 assert_ne!(exit_code, 0);
        //                 actual_error = Some(D::decode(&mut message.payload())
        //                     .expect("Unable to decode bytes from error reply"));
        //             }
        //         }
        //     }
        // });

        // let actual_error = actual_error.expect("Error message not found in any `Event::UserMessageSent`");

        // assert_eq!(error, actual_error)
    }

    pub(super) fn assert_not_executed(message_id: MessageId) {
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::NotExecuted)
    }

    pub(super) fn get_last_program_id() -> ProgramId {
        let event = match SystemPallet::<Test>::events()
            .last()
            .map(|r| r.event.clone())
        {
            Some(MockEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        if let Event::MessageEnqueued {
            destination,
            entry: Entry::Init,
            ..
        } = event
        {
            destination
        } else {
            unreachable!("expect Event::InitMessageEnqueued")
        }
    }

    pub(super) fn get_last_message_id() -> MessageId {
        SystemPallet::<Test>::events()
            .iter()
            .rev()
            .filter_map(|r| {
                if let MockEvent::Gear(e) = r.event.clone() {
                    Some(e)
                } else {
                    None
                }
            })
            .find_map(|e| match e {
                Event::MessageEnqueued { id, .. } => Some(id),
                Event::UserMessageSent { message, .. } => Some(message.id()),
                _ => None,
            })
            .expect("can't find message send event")
    }

    pub(super) fn get_last_mail(account: AccountId) -> StoredMessage {
        MailboxOf::<Test>::iter_key(account)
            .last()
            .expect("Element should be")
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
                    // "Not enough gas to continue execution" a.k.a. "Gas limit exceeded"
                    // execution outcome error message.
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

    pub(super) fn waiting_init_messages(pid: ProgramId) -> Vec<MessageId> {
        let key = common::waiting_init_prefix(pid);
        sp_io::storage::get(&key)
            .and_then(|v| Vec::<MessageId>::decode(&mut &v[..]).ok())
            .unwrap_or_default()
    }
}
