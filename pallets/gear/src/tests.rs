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

use core::convert::TryInto;

use crate::{
    internal::HoldBound,
    manager::HandleKind,
    mock::{
        self,
        new_test_ext,
        run_to_block,
        run_to_next_block,
        Balances,
        Gear,
        // Randomness,
        RuntimeEvent as MockRuntimeEvent,
        RuntimeOrigin,
        System,
        Test,
        Timestamp,
        BLOCK_AUTHOR,
        LOW_BALANCE_USER,
        USER_1,
        USER_2,
        USER_3,
    },
    pallet, BlockGasLimitOf, Config, CostsPerBlockOf, DbWeightOf, Error, Event, GasAllowanceOf,
    GasHandlerOf, GasInfo, MailboxOf, ProgramStorageOf, Schedule, TaskPoolOf, WaitlistOf,
};
use common::{
    event::*, scheduler::*, storage::*, CodeStorage, GasPrice as _, GasTree, Origin as _,
    ProgramStorage,
};
use core_processor::{common::ActorExecutionErrorReason, ActorPrepareMemoryError};
use demo_compose::WASM_BINARY as COMPOSE_WASM_BINARY;
use demo_mul_by_const::WASM_BINARY as MUL_CONST_WASM_BINARY;
use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};
use demo_waiting_proxy::WASM_BINARY as WAITING_PROXY_WASM_BINARY;
use frame_support::{
    assert_noop, assert_ok,
    codec::{Decode, Encode},
    dispatch::Dispatchable,
    sp_runtime::traits::{TypedGet, Zero},
    traits::{Currency, Randomness},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_backend_common::TrapExplanation;
use gear_core::{
    code::{self, Code},
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageU32Size, WasmPage},
};
use gear_core_errors::*;
use gear_wasm_instrument::STACK_END_EXPORT_NAME;
use sp_runtime::{traits::UniqueSaturatedInto, SaturatedConversion};
use sp_std::convert::TryFrom;
pub use utils::init_logger;
use utils::*;

type Gas = <<Test as Config>::GasProvider as common::GasProvider>::GasTree;

#[test]
fn gasfull_after_gasless() {
    init_logger();

    let wat = format!(
        r#"
        (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_reply_wgas" (func $reply_wgas (param i32 i32 i64 i32 i32 i32)))
        (import "env" "gr_send" (func $send (param i32 i32 i32 i32 i32)))
        (export "init" (func $init))
        (func $init
            i32.const 111 ;; ptr
            i32.const 1 ;; value
            i32.store

            (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i32.const 10) (i32.const 333))
            (call $reply_wgas (i32.const 0) (i32.const 32) (i64.const {gas_limit}) (i32.const 222) (i32.const 10) (i32.const 333))
        )
    )"#,
        gas_limit = 10 * <Test as Config>::MailboxThreshold::get()
    );

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(&wat).to_bytes();

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(code.clone()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            min_limit - 1,
            0,
        ));

        // Make sure nothing panics.
        run_to_next_block(None);
    })
}

#[test]
fn backend_errors_handled_in_program() {
    use demo_backend_error::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let mid = utils::get_last_message_id();

        run_to_next_block(None);
        // If nothing panicked, so program's logic and backend are correct.
        utils::assert_succeed(mid);
    })
}

#[test]
fn non_existent_code_id_zero_gas() {
    init_logger();

    let wat = r#"
    (module
    (import "env" "memory" (memory 1))
    (import "env" "gr_create_program_wgas" (func $create_program_wgas (param i32 i32 i32 i32 i32 i64 i32 i32)))
    (export "init" (func $init))
    (func $init
        i32.const 0     ;; zeroed cid_value ptr
        i32.const 0     ;; salt ptr
        i32.const 0     ;; salt len
        i32.const 0     ;; payload ptr
        i32.const 0     ;; payload len
        i64.const 0     ;; gas limit
        i32.const 0     ;; delay
        i32.const 111               ;; err_mid_pid ptr
        call $create_program_wgas   ;; calling fn

        ;; validating syscall
        i32.const 111 ;; err_mid_pid ptr
        i32.load
        (if
            (then unreachable)
            (else)
        )
    )
 )"#;

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        run_to_next_block(None);

        // Nothing panics here.
        //
        // 1st msg is init of "factory"
        // 2nd is init of non existing code id
        // 3rd is error reply on 2nd message
        assert_total_dequeued(3);
    })
}

#[test]
fn waited_with_zero_gas() {
    init_logger();

    let wat = r#"
    (module
    (import "env" "memory" (memory 1))
    (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
    (import "env" "gr_wait_for" (func $wait_for (param i32)))
    (import "env" "gr_exit" (func $exit (param i32)))
    (export "init" (func $init))
    (export "handle_reply" (func $handle_reply))
    (func $init
        i32.const 111 ;; ptr
        i32.const 1 ;; value
        i32.store

        (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i64.const 12345) (i32.const 0) (i32.const 333))

        ;; validating syscall
        i32.const 333 ;; err_mid ptr
        i32.load
        (if
            (then unreachable)
            (else)
        )

        (call $wait_for (i32.const 2))
    )
    (func $handle_reply
        (call $exit (i32.const 111))
    )
 )"#;

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(code.clone()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            min_limit,
            0,
        ));

        let program_id = utils::get_last_program_id();

        // Check that block number matches program upload block number
        let upload_block_number = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), upload_block_number);

        run_to_next_block(None);
        let mid_in_mailbox = utils::get_last_message_id();

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            mid_in_mailbox,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        run_to_next_block(None);
        assert!(Gear::is_exited(program_id));

        // Check that block number matches block number when program exited
        let exited_block_number = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), exited_block_number);

        // Nothing panics here.
        //
        // Twice for init message.
        // Once for reply sent.
        assert_total_dequeued(3);
    })
}

#[test]
fn terminated_program_zero_gas() {
    init_logger();

    let wat = r#"
    (module
    (import "env" "memory" (memory 0))
    (export "init" (func $init))
    (func $init
        unreachable
    )
 )"#;

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let program_id = utils::get_last_program_id();

        // Check that block number matches program upload block number
        let upload_block_number = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), upload_block_number);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            0,
            0,
        ));

        run_to_next_block(None);
        assert!(Gear::is_terminated(program_id));

        // Check that block number matches block number when program terminated
        let terminated_block_number = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), terminated_block_number);

        // Nothing panics here.
        assert_total_dequeued(2);
    })
}

#[test]
fn exited_program_zero_gas() {
    init_logger();

    let wat = r#"
    (module
    (import "env" "memory" (memory 1))
    (import "env" "gr_exit" (func $exit (param i32)))
    (export "init" (func $init))
    (func $init
        i32.const 0
        call $exit
    )
 )"#;

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let program_id = utils::get_last_program_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            0,
            0,
        ));

        run_to_next_block(None);
        assert!(Gear::is_exited(program_id));

        // Nothing panics here.
        assert_total_dequeued(2);
    })
}

#[test]
fn delayed_user_replacement() {
    use demo_proxy_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    fn scenario(gas_limit_to_forward: u64, to_mailbox: bool) {
        let code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
        let future_program_address = ProgramId::generate(CodeId::generate(&code), DEFAULT_SALT);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: <[u8; 32]>::from(future_program_address).into(),
                delay: 1,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            gas_limit_to_forward.encode(), // to be forwarded as gas limit
            gas_limit_to_forward + DEFAULT_GAS_LIMIT * 100,
            100_000_000, // before fix to be forwarded as value
        ));

        let message_id = utils::get_last_message_id();
        let delayed_id = MessageId::generate_outgoing(message_id, 0);

        run_to_block(3, None);

        // Message sending delayed.
        assert!(TaskPoolOf::<Test>::contains(
            &4,
            &ScheduledTask::SendUserMessage {
                message_id: delayed_id,
                to_mailbox
            }
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        assert_eq!(future_program_address, utils::get_last_program_id());

        run_to_next_block(None);
        assert!(Gear::is_initialized(future_program_address));

        // Delayed message sent.
        assert!(!TaskPoolOf::<Test>::contains(
            &4,
            &ScheduledTask::SendUserMessage {
                message_id: delayed_id,
                to_mailbox
            }
        ));

        // Replace following lines once added validation to task handling of send_user_message.
        let message = utils::maybe_any_last_message().unwrap();
        assert_eq!(message.id(), delayed_id);
        assert_eq!(message.destination(), future_program_address);

        print_gear_events();

        // BELOW CODE TO REPLACE WITH.
        // // Nothing is added into mailbox.
        // assert!(utils::maybe_any_last_message(account).is_empty())

        // // Error reply sent and processed.
        // assert_total_dequeued(1);
    }

    init_logger();

    // Scenario not planned to enter mailbox.
    new_test_ext().execute_with(|| scenario(0, false));

    // Scenario planned to enter mailbox.
    new_test_ext().execute_with(|| {
        let gas_limit_to_forward = DEFAULT_GAS_LIMIT * 100;
        assert!(<Test as Config>::MailboxThreshold::get() <= gas_limit_to_forward);

        scenario(gas_limit_to_forward, true)
    });
}

#[test]
fn delayed_send_user_message_payment() {
    use demo_proxy_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    // Testing that correct gas amount will be reserved and payed for holding.
    fn scenario(delay: u64) {
        // Upload program that sends message to any user.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: USER_2.into(),
                delay: delay as u32,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy_msg_id = get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = GasPrice::gas_price(
            delay
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let reserve_for_fee = GasPrice::gas_price(
            CostsPerBlockOf::<Test>::reserve_for()
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        // Gas should be reserved until message is holding.
        assert_eq!(Balances::reserved_balance(USER_1), delay_holding_fee);
        let total_balance = Balances::free_balance(USER_1) + Balances::reserved_balance(USER_1);

        // Run blocks before sending message.
        run_to_block(delay + 2, None);

        let delayed_id = MessageId::generate_outgoing(proxy_msg_id, 0);

        // Check that delayed task was created.
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendUserMessage {
                message_id: delayed_id,
                to_mailbox: false
            }
        ));

        // Mailbox should be empty.
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        run_to_next_block(None);

        // Check that last event is UserMessageSent.
        let last_event = match get_last_event() {
            MockRuntimeEvent::Gear(e) => e,
            _ => panic!("Should be one Gear event"),
        };
        match last_event {
            Event::UserMessageSent { message, .. } => assert_eq!(delayed_id, message.id()),
            _ => panic!("Test failed: expected Event::UserMessageSent"),
        }

        // Mailbox should be empty.
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        // Check balances match and gas charging is correct.
        assert_eq!(Balances::reserved_balance(USER_1), 0);
        assert_eq!(
            total_balance - delay_holding_fee + reserve_for_fee,
            Balances::free_balance(USER_1)
        );
    }

    init_logger();

    for i in 2..4 {
        new_test_ext().execute_with(|| scenario(i));
    }
}

#[test]
fn delayed_send_user_message_with_reservation() {
    use demo_proxy_reservation_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    // Testing that correct gas amount will be reserved and payed for holding.
    fn scenario(delay: u64) {
        let reservation_amount = 6_000_000_000u64;

        // Upload program that sends message to any user.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: USER_2.into(),
                delay: delay as u32,
                reservation_amount,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy_msg_id = get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = GasPrice::gas_price(
            delay
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let reserve_for_fee = GasPrice::gas_price(
            CostsPerBlockOf::<Test>::reserve_for()
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let mailbox_gas_threshold = GasPrice::gas_price(<Test as Config>::MailboxThreshold::get());

        // Gas should be reserved until message is holding.
        assert_eq!(
            Balances::reserved_balance(USER_1),
            mailbox_gas_threshold + delay_holding_fee
        );

        // Run blocks before sending message.
        run_to_block(delay + 2, None);

        let delayed_id = MessageId::generate_outgoing(proxy_msg_id, 0);

        // Check that delayed task was created.
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendUserMessage {
                message_id: delayed_id,
                to_mailbox: true
            }
        ));

        // Mailbox should be empty.
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        run_to_next_block(None);

        // Check that last event is UserMessageSent.
        let last_event = match get_last_event() {
            MockRuntimeEvent::Gear(e) => e,
            _ => panic!("Should be one Gear event"),
        };
        match last_event {
            Event::UserMessageSent { message, .. } => assert_eq!(delayed_id, message.id()),
            _ => panic!("Test failed: expected Event::UserMessageSent"),
        }

        // Mailbox should not be empty.
        assert!(!MailboxOf::<Test>::is_empty(&USER_2));

        // TODO: deal with reserve_for in reserve.
        assert_eq!(
            Balances::reserved_balance(USER_1),
            mailbox_gas_threshold + reserve_for_fee
        );
    }

    init_logger();

    for i in 2..4 {
        new_test_ext().execute_with(|| scenario(i));
    }
}

#[test]
fn delayed_send_program_message_payment() {
    use demo_proxy_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    // Testing that correct gas amount will be reserved and payed for holding.
    fn scenario(delay: u64) {
        // Upload empty program that receive the message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let program_address = utils::get_last_program_id();

        // Upload program that sends message to another program.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: <[u8; 32]>::from(program_address).into(),
                delay: delay as u32,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));
        assert!(Gear::is_initialized(program_address));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));
        let proxy_msg_id = utils::get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = GasPrice::gas_price(
            delay
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let reserve_for_fee = GasPrice::gas_price(
            CostsPerBlockOf::<Test>::reserve_for()
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        // Gas should be reserved until message is holding.
        assert_eq!(Balances::reserved_balance(USER_1), delay_holding_fee);
        let total_balance = Balances::free_balance(USER_1) + Balances::reserved_balance(USER_1);

        // Run blocks to release message.
        run_to_block(delay + 2, None);

        let delayed_id = MessageId::generate_outgoing(proxy_msg_id, 0);

        // Check that delayed task was created.
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Block where message processed.
        run_to_next_block(None);

        // Check that last event is MessagesDispatched.
        assert_last_dequeued(2);

        // Check that gas was charged correctly.
        assert_eq!(Balances::reserved_balance(USER_1), 0);
        assert_eq!(
            total_balance - delay_holding_fee + reserve_for_fee,
            Balances::free_balance(USER_1)
        );
    }

    init_logger();

    for i in 2..4 {
        new_test_ext().execute_with(|| scenario(i));
    }
}

#[test]
fn delayed_send_program_message_with_reservation() {
    use demo_proxy_reservation_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    // Testing that correct gas amount will be reserved and payed for holding.
    fn scenario(delay: u64) {
        // Upload empty program that receive the message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let program_address = utils::get_last_program_id();
        let reservation_amount = 6_000_000_000u64;

        // Upload program that sends message to another program.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: <[u8; 32]>::from(program_address).into(),
                delay: delay as u32,
                reservation_amount,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));
        assert!(Gear::is_initialized(program_address));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));
        let proxy_msg_id = utils::get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = GasPrice::gas_price(
            delay
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let reservation_holding_fee = GasPrice::gas_price(
            80u64
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::reservation()),
        );

        let delayed_id = MessageId::generate_outgoing(proxy_msg_id, 0);

        // Check that delayed task was created
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Check that correct amount locked for dispatch stash
        let gas_locked_in_gas_node = GasPrice::gas_price(Gas::get_lock(delayed_id).unwrap());
        assert_eq!(gas_locked_in_gas_node, delay_holding_fee);

        // Gas should be reserved until message is holding.
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(reservation_amount) + reservation_holding_fee
        );

        // Run blocks to release message.
        run_to_block(delay + 2, None);

        // Check that delayed task was created
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Block where message processed
        run_to_next_block(None);

        // Check that last event is MessagesDispatched.
        assert_last_dequeued(2);

        assert_eq!(Balances::reserved_balance(USER_1), 0);
    }

    init_logger();

    for i in 2..4 {
        new_test_ext().execute_with(|| scenario(i));
    }
}

#[test]
fn delayed_send_program_message_with_low_reservation() {
    use demo_proxy_reservation_with_gas::{InputArgs, WASM_BINARY as PROXY_WGAS_WASM_BINARY};

    // Testing that correct gas amount will be reserved and payed for holding.
    fn scenario(delay: u64) {
        // Upload empty program that receive the message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let program_address = utils::get_last_program_id();
        let reservation_amount = <Test as Config>::MailboxThreshold::get();

        // Upload program that sends message to another program.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: <[u8; 32]>::from(program_address).into(),
                delay: delay as u32,
                reservation_amount,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(proxy));
        assert!(Gear::is_initialized(program_address));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));
        let proxy_msg_id = utils::get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = GasPrice::gas_price(
            delay
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        let reservation_holding_fee = GasPrice::gas_price(
            80u64
                .saturating_add(CostsPerBlockOf::<Test>::reserve_for().unique_saturated_into())
                .saturating_mul(CostsPerBlockOf::<Test>::reservation()),
        );

        let delayed_id = MessageId::generate_outgoing(proxy_msg_id, 0);

        // Check that delayed task was created
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Check that correct amount locked for dispatch stash
        let gas_locked_in_gas_node = GasPrice::gas_price(Gas::get_lock(delayed_id).unwrap());
        assert_eq!(gas_locked_in_gas_node, delay_holding_fee);

        // Gas should be reserved until message is holding.
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(reservation_amount) + reservation_holding_fee
        );

        // Run blocks to release message.
        run_to_block(delay + 2, None);

        // Check that delayed task was created
        assert!(TaskPoolOf::<Test>::contains(
            &(delay + 3),
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Block where message processed
        run_to_next_block(None);

        // Check that last event is MessagesDispatched.
        assert_last_dequeued(2);

        assert_eq!(Balances::reserved_balance(USER_1), 0);
    }

    init_logger();

    for i in 2..4 {
        new_test_ext().execute_with(|| scenario(i));
    }
}

#[test]
fn delayed_program_creation_no_code() {
    init_logger();

    let wat = r#"
	(module
		(import "env" "memory" (memory 1))
        (import "env" "gr_create_program_wgas" (func $create_program_wgas (param i32 i32 i32 i32 i32 i64 i32 i32)))
		(export "init" (func $init))
		(func $init
            i32.const 0                 ;; zeroed cid_value ptr
            i32.const 0                 ;; salt ptr
            i32.const 0                 ;; salt len
            i32.const 0                 ;; payload ptr
            i32.const 0                 ;; payload len
            i64.const 1000000000        ;; gas limit
            i32.const 1                 ;; delay
            i32.const 111               ;; err_mid_pid ptr
            call $create_program_wgas   ;; calling fn

            ;; validating syscall
            i32.const 111 ;; err_mid_pid ptr
            i32.load
            (if
                (then unreachable)
                (else)
            )
        )
	)"#;

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
        ));

        let creator = utils::get_last_program_id();
        let init_msg_id = utils::get_last_message_id();

        run_to_block(2, None);
        assert!(Gear::is_initialized(creator));

        // Message sending delayed.
        let delayed_id = MessageId::generate_outgoing(init_msg_id, 0);
        assert!(TaskPoolOf::<Test>::contains(
            &3,
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        let free_balance = Balances::free_balance(USER_1);
        let reserved_balance = Balances::reserved_balance(USER_1);

        run_to_next_block(None);
        // Delayed message sent.
        assert!(!TaskPoolOf::<Test>::contains(
            &3,
            &ScheduledTask::SendDispatch(delayed_id)
        ));

        // Message taken but not executed (can't be asserted due to black box between programs).
        //
        // Total dequeued: message to skip execution + error reply on it.
        //
        // Single db read burned for querying program data from storage.
        assert_last_dequeued(2);

        let delayed_block_amount: u64 = 1;

        let delay_holding_fee = GasPrice::gas_price(
            delayed_block_amount.saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );

        assert_eq!(
            Balances::free_balance(USER_1),
            free_balance + reserved_balance
                - delay_holding_fee
                - GasPrice::gas_price(DbWeightOf::<Test>::get().reads(1).ref_time())
        );
        assert!(Balances::reserved_balance(USER_1).is_zero());
    })
}

#[test]
fn unstoppable_block_execution_works() {
    init_logger();

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let user_balance = Balances::free_balance(USER_1);
        let user_gas = Balances::free_balance(USER_1) as u64 / 1_000;

        // This manipulations are required due to we have only gas to value conversion.
        assert_eq!(GasPrice::gas_price(user_gas), user_balance);
        let executions_amount = 100;
        let gas_for_each_execution = user_gas / executions_amount;

        assert!(gas_for_each_execution < BlockGasLimitOf::<Test>::get());

        let program_id = {
            let res = upload_program_default(USER_2, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, None);

        let GasInfo {
            burned: expected_burned_gas,
            may_be_returned,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_for_each_execution > expected_burned_gas);

        for _ in 0..executions_amount {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                program_id,
                EMPTY_PAYLOAD.to_vec(),
                gas_for_each_execution,
                0,
            ));
        }

        let real_gas_to_burn = expected_burned_gas
            + executions_amount.saturating_sub(1) * (expected_burned_gas - may_be_returned);

        assert!(gas_for_each_execution * executions_amount > real_gas_to_burn);

        run_to_block(3, Some(minimal_weight.ref_time() + real_gas_to_burn));

        assert_last_dequeued(executions_amount as u32);

        assert_eq!(GasAllowanceOf::<Test>::get(), 0);

        assert_eq!(
            Balances::free_balance(USER_1),
            GasPrice::gas_price(user_gas - real_gas_to_burn)
        );
    })
}

#[test]
fn read_state_works() {
    use demo_new_meta::{MessageInitIn, Wallet, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            <MessageInitIn as Default>::default().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));

        let expected = Wallet::test_sequence().encode();

        let res = Gear::read_state_impl(program_id).expect("Failed to read state");

        assert_eq!(res, expected);
    });
}

#[test]
fn read_state_using_wasm_works() {
    use demo_new_meta::{
        Id, MessageInitIn, Wallet, META_EXPORTS_V1, META_EXPORTS_V2, META_WASM_V1, META_WASM_V2,
        WASM_BINARY,
    };

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            <MessageInitIn as Default>::default().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));

        let expected = Wallet::test_sequence().into_iter().last().encode();

        let func1 = "last_wallet";
        assert!(META_EXPORTS_V1.contains(&func1));

        let res = Gear::read_state_using_wasm_impl(program_id, func1, META_WASM_V1.to_vec(), None)
            .expect("Failed to read state");

        assert_eq!(res, expected);

        let id = Id {
            decimal: 1,
            hex: vec![1],
        };

        let expected = Wallet::test_sequence()
            .into_iter()
            .find(|w| w.id == id)
            .encode();

        let func2 = "wallet_by_id";
        assert!(META_EXPORTS_V2.contains(&func2));
        assert!(!META_EXPORTS_V2.contains(&func1));

        let res = Gear::read_state_using_wasm_impl(
            program_id,
            func2,
            META_WASM_V2.to_vec(),
            Some(id.encode()),
        )
        .expect("Failed to read state");

        assert_eq!(res, expected);
    });
}

#[test]
fn read_state_bn_and_timestamp_works() {
    use demo_new_meta::{MessageInitIn, META_WASM_V3, WASM_BINARY};

    let check = |program_id: ProgramId| {
        let expected: u32 = Gear::block_number().unique_saturated_into();

        let res = Gear::read_state_using_wasm_impl(
            program_id,
            "block_number",
            META_WASM_V3.to_vec(),
            None,
        )
        .expect("Failed to read state");
        let res = u32::decode(&mut res.as_ref()).unwrap();

        assert_eq!(res, expected);

        let expected: u64 = Timestamp::get().unique_saturated_into();

        let res = Gear::read_state_using_wasm_impl(
            program_id,
            "block_timestamp",
            META_WASM_V3.to_vec(),
            None,
        )
        .expect("Failed to read state");
        let res = u64::decode(&mut res.as_ref()).unwrap();

        assert_eq!(res, expected);
    };

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            <MessageInitIn as Default>::default().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(program_id));
        check(program_id);

        run_to_block(10, None);
        check(program_id);

        run_to_block(20, None);
        check(program_id);
    });
}

#[test]
fn wasm_metadata_generation_works() {
    use demo_new_meta::{
        MessageInitIn, META_EXPORTS_V1, META_EXPORTS_V2, META_WASM_V1, META_WASM_V2, WASM_BINARY,
    };

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            <MessageInitIn as Default>::default().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));

        let m1 =
            Gear::read_state_using_wasm_impl(program_id, "metadata", META_WASM_V1.to_vec(), None)
                .expect("Failed to read state");

        let metadata1 =
            gmeta::MetawasmData::decode(&mut m1.as_ref()).expect("Failed to decode metadata");
        let mut exports1 = metadata1.funcs.keys().cloned().collect::<Vec<_>>();
        exports1.push("metadata".into());
        exports1.sort();
        let mut expected_exports_1 = META_EXPORTS_V1.to_vec();
        expected_exports_1.sort();
        assert_eq!(exports1, expected_exports_1);

        let m2 =
            Gear::read_state_using_wasm_impl(program_id, "metadata", META_WASM_V2.to_vec(), None)
                .expect("Failed to read state");

        let metadata2 =
            gmeta::MetawasmData::decode(&mut m2.as_ref()).expect("Failed to decode metadata");
        let mut exports2 = metadata2.funcs.keys().cloned().collect::<Vec<_>>();
        exports2.push("metadata".into());
        exports2.sort();
        let mut expected_exports_2 = META_EXPORTS_V2.to_vec();
        expected_exports_2.sort();
        assert_eq!(exports2, expected_exports_2);
    });
}

#[test]
fn read_state_using_wasm_errors() {
    use demo_new_meta::{MessageInitIn, WASM_BINARY};

    let wat = r#"
	(module
		(export "loop" (func $loop))
        (export "empty" (func $empty))
        (func $empty)
        (func $loop
            (loop)
        )
	)"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let meta_wasm = ProgramCodeKind::Custom(wat).to_bytes().to_vec();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            <MessageInitIn as Default>::default().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(program_id));

        // Inexistent function
        assert!(Gear::read_state_using_wasm_impl(
            program_id,
            "inexistent",
            meta_wasm.clone(),
            None
        )
        .is_err());
        // Empty function
        assert!(
            Gear::read_state_using_wasm_impl(program_id, "empty", meta_wasm.clone(), None).is_err()
        );
        // Greed function
        assert!(Gear::read_state_using_wasm_impl(program_id, "loop", meta_wasm, None).is_err());
    });
}

#[test]
fn mailbox_rent_out_of_rent() {
    use demo_value_sender::{TestData, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let sender = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(sender));

        // Message removes due to out of rent condition.
        //
        // For both cases value moves back to program.
        let cases = [
            // Gasful message.
            TestData::gasful(20_000, 1_000),
            // Gasless message.
            TestData::gasless(3_000, <Test as Config>::MailboxThreshold::get()),
        ];

        let mb_cost = CostsPerBlockOf::<Test>::mailbox();
        let reserve_for = CostsPerBlockOf::<Test>::reserve_for();

        for data in cases {
            let user_1_balance = Balances::free_balance(USER_1);
            assert_eq!(Balances::reserved_balance(USER_1), 0);

            let user_2_balance = Balances::free_balance(USER_2);
            assert_eq!(Balances::reserved_balance(USER_2), 0);

            let prog_balance = Balances::free_balance(AccountId::from_origin(sender.into_origin()));
            assert_eq!(
                Balances::reserved_balance(AccountId::from_origin(sender.into_origin())),
                0
            );

            let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
                USER_1,
                sender,
                data.request(USER_2).encode(),
                Some(data.extra_gas),
                0,
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
                GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            let hold_bound = HoldBound::<Test>::by(CostsPerBlockOf::<Test>::mailbox())
                .maximum_for(data.gas_limit_to_send);

            let expected_duration = data.gas_limit_to_send / mb_cost - reserve_for;

            assert_eq!(
                hold_bound.expected_duration(),
                expected_duration.saturated_into::<BlockNumberFor<Test>>()
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.burned + data.gas_limit_to_send),
                GasPrice::gas_price(data.gas_limit_to_send),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, data.value);
            assert!(!MailboxOf::<Test>::is_empty(&USER_2));

            run_to_block(hold_bound.expected(), None);

            let gas_totally_burned =
                gas_info.burned + data.gas_limit_to_send - reserve_for * mb_cost;

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_totally_burned),
                0u128,
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));
        }
    });
}

#[test]
fn mailbox_rent_claimed() {
    use demo_value_sender::{TestData, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let sender = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(sender));

        // Message removes due to claim.
        //
        // For both cases value moves to destination user.
        let cases = [
            // Gasful message and 10 blocks of hold in mailbox.
            (TestData::gasful(20_000, 1_000), 10),
            // Gasless message and 5 blocks of hold in mailbox.
            (
                TestData::gasless(3_000, <Test as Config>::MailboxThreshold::get()),
                5,
            ),
        ];

        let mb_cost = CostsPerBlockOf::<Test>::mailbox();

        for (data, duration) in cases {
            let user_1_balance = Balances::free_balance(USER_1);
            assert_eq!(Balances::reserved_balance(USER_1), 0);

            let user_2_balance = Balances::free_balance(USER_2);
            assert_eq!(Balances::reserved_balance(USER_2), 0);

            let prog_balance = Balances::free_balance(AccountId::from_origin(sender.into_origin()));
            assert_eq!(
                Balances::reserved_balance(AccountId::from_origin(sender.into_origin())),
                0
            );

            let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
                USER_1,
                sender,
                data.request(USER_2).encode(),
                Some(data.extra_gas),
                0,
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
                GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            let message_id = utils::get_last_message_id();

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.burned + data.gas_limit_to_send),
                GasPrice::gas_price(data.gas_limit_to_send),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, data.value);
            assert!(!MailboxOf::<Test>::is_empty(&USER_2));

            run_to_block(
                Gear::block_number() + duration.saturated_into::<BlockNumberFor<Test>>(),
                None,
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.burned + data.gas_limit_to_send),
                GasPrice::gas_price(data.gas_limit_to_send),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, data.value);
            assert!(!MailboxOf::<Test>::is_empty(&USER_2));

            assert_ok!(Gear::claim_value(RuntimeOrigin::signed(USER_2), message_id));

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.burned + duration * mb_cost),
                0u128,
            );
            utils::assert_balance(USER_2, user_2_balance + data.value, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));
        }
    });
}

#[test]
fn mailbox_sending_instant_transfer() {
    use demo_value_sender::{SendingRequest, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let sender = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(sender));

        // Message doesn't add to mailbox.
        //
        // For both cases value moves to destination user instantly.
        let cases = [
            // Zero gas for gasful sending.
            (Some(0), 1_000),
            // Gasless message.
            (None, 3_000),
        ];

        for (gas_limit, value) in cases {
            let user_1_balance = Balances::free_balance(USER_1);
            assert_eq!(Balances::reserved_balance(USER_1), 0);

            let user_2_balance = Balances::free_balance(USER_2);
            assert_eq!(Balances::reserved_balance(USER_2), 0);

            let prog_balance = Balances::free_balance(AccountId::from_origin(sender.into_origin()));
            assert_eq!(
                Balances::reserved_balance(AccountId::from_origin(sender.into_origin())),
                0
            );

            let payload = if let Some(gas_limit) = gas_limit {
                SendingRequest::gasful(USER_2, gas_limit, value)
            } else {
                SendingRequest::gasless(USER_2, value)
            };

            // Used like that, because calculate gas info always provides
            // message into mailbox while sending without gas.
            let gas_info = Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(sender),
                payload.clone().encode(),
                0,
                true,
            )
            .expect("calculate_gas_info failed");

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                sender,
                payload.encode(),
                gas_info.burned + gas_limit.unwrap_or_default(),
                0
            ));

            utils::assert_balance(
                USER_1,
                user_1_balance
                    - GasPrice::gas_price(gas_info.burned + gas_limit.unwrap_or_default()),
                GasPrice::gas_price(gas_info.burned + gas_limit.unwrap_or_default()),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            utils::assert_balance(
                USER_1,
                user_1_balance - GasPrice::gas_price(gas_info.burned),
                0u128,
            );
            utils::assert_balance(USER_2, user_2_balance + value, 0u128);
            utils::assert_balance(sender, prog_balance - value, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));
        }
    });
}

#[test]
fn upload_program_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let balance = Balances::free_balance(USER_1);
        assert_noop!(
            Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::Default.to_bytes(),
                DEFAULT_SALT.to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                balance + 1
            ),
            Error::<Test>::InsufficientBalanceForReserve
        );

        assert_noop!(
            upload_program_default(LOW_BALANCE_USER, ProgramCodeKind::Default),
            Error::<Test>::InsufficientBalanceForReserve
        );

        // Gas limit is too high
        let block_gas_limit = BlockGasLimitOf::<Test>::get();
        assert_noop!(
            Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
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
fn upload_program_fails_on_duplicate_id() {
    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(upload_program_default(USER_1, ProgramCodeKind::Default));
        // Finalize block to let queue processing run
        run_to_block(2, None);
        // By now this program id is already in the storage
        assert_noop!(
            upload_program_default(USER_1, ProgramCodeKind::Default),
            Error::<Test>::ProgramAlreadyExists
        );
    })
}

#[test]
fn send_message_works() {
    init_logger();

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let user1_initial_balance = Balances::free_balance(USER_1);
        let user2_initial_balance = Balances::free_balance(USER_2);

        // No gas has been created initially
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_ok!(send_default_message(USER_1, program_id));

        // Balances check
        // Gas spends on sending 2 default messages (submit program and send message to program)
        let user1_potential_msgs_spends = GasPrice::gas_price(2 * DEFAULT_GAS_LIMIT);
        // User 1 has sent two messages
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );

        // Clear messages from the queue to refund unused gas
        run_to_block(2, None);

        // Checking that sending a message to a non-program address works as a value transfer
        let mail_value = 20_000;

        // Take note of up-to-date users balance
        let user1_initial_balance = Balances::free_balance(USER_1);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            USER_2.into(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            mail_value,
        ));
        let message_id = get_last_message_id();

        // Transfer of `mail_value` completed.
        // Gas limit is ignored for messages headed to a mailbox - no funds have been reserved.
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - mail_value
        );
        // The recipient has received the funds.
        // Interaction between users doesn't affect mailbox.
        assert_eq!(
            Balances::free_balance(USER_2),
            user2_initial_balance + mail_value
        );

        assert!(!MailboxOf::<Test>::contains(&USER_2, &message_id));

        // Ensure the message didn't burn any gas (i.e. never went through processing pipeline)
        let remaining_weight = 100_000;
        run_to_block(3, Some(remaining_weight));

        // Messages were sent by user 1 only
        let actual_gas_burned =
            remaining_weight - minimal_weight.ref_time() - GasAllowanceOf::<Test>::get();
        assert_eq!(actual_gas_burned, 0);

        // Ensure that no gas handlers were created
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    });
}

#[test]
fn mailbox_threshold_works() {
    use demo_proxy_with_gas::{InputArgs, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            InputArgs {
                destination: USER_1.into_origin().into(),
                delay: 0,
            }
            .encode(),
            50_000_000_000u64,
            0u128
        ));

        let proxy = utils::get_last_program_id();
        let rent = <Test as Config>::MailboxThreshold::get();
        let check_result = |sufficient: bool| -> MessageId {
            run_to_next_block(None);

            let mailbox_key = AccountId::from_origin(USER_1.into_origin());
            let message_id = get_last_message_id();

            if sufficient {
                // * message has been inserted into the mailbox.
                // * the ValueNode has been created.
                assert!(MailboxOf::<Test>::contains(&mailbox_key, &message_id));
                assert_ok!(GasHandlerOf::<Test>::get_limit(message_id), rent);
            } else {
                // * message has not been inserted into the mailbox.
                // * the ValueNode has not been created.
                assert!(!MailboxOf::<Test>::contains(&mailbox_key, &message_id));
                assert_noop!(
                    GasHandlerOf::<Test>::get_limit(message_id),
                    pallet_gear_gas::Error::<Test>::NodeNotFound
                );
            }

            message_id
        };

        // send message with insufficient message rent
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            (rent - 1).encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));
        check_result(false);

        // // send message with enough gas_limit
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            (rent).encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));
        let message_id = check_result(true);

        // send reply with enough gas_limit
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            rent.encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));
        let message_id = check_result(true);

        // send reply with insufficient message rent
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            (rent - 1).encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));
        check_result(false);
    })
}

#[test]
fn send_message_uninitialized_program() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Submitting program and send message until it's uninitialized
        // Submitting first program and getting its id
        let code = ProgramCodeKind::Default.to_bytes();
        let salt = DEFAULT_SALT.to_vec();

        let program_id = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        assert!(Gear::is_active(program_id));
        assert!(!Gear::is_initialized(program_id));

        // Sending message while program is still not initialized
        assert_ok!(call_default_message(program_id).dispatch(RuntimeOrigin::signed(USER_1)));
        let message_id = get_last_message_id();

        run_to_block(2, None);

        assert_succeed(message_id);

        assert!(Gear::is_initialized(program_id));
    })
}

#[test]
fn send_message_expected_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Submitting failing in init program and check message is failed to be sent to it
        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, None);

        assert_noop!(
            call_default_message(program_id).dispatch(RuntimeOrigin::signed(LOW_BALANCE_USER)),
            Error::<Test>::InactiveProgram
        );

        // Submit valid program and test failing actions on it
        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_noop!(
            call_default_message(program_id).dispatch(RuntimeOrigin::signed(LOW_BALANCE_USER)),
            Error::<Test>::InsufficientBalanceForReserve
        );

        let low_balance_user_balance = Balances::free_balance(LOW_BALANCE_USER);
        let user_1_balance = Balances::free_balance(USER_1);
        let value = 1000;

        // Because destination is user, no gas will be reserved
        MailboxOf::<Test>::clear();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(LOW_BALANCE_USER),
            USER_1.into(),
            EMPTY_PAYLOAD.to_vec(),
            10,
            value
        ));

        // And no message will be in mailbox
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // Value transfers immediately.
        assert_eq!(
            low_balance_user_balance - value,
            Balances::free_balance(LOW_BALANCE_USER)
        );
        assert_eq!(user_1_balance + value, Balances::free_balance(USER_1));

        // Gas limit too high
        let block_gas_limit = BlockGasLimitOf::<Test>::get();
        assert_noop!(
            Gear::send_message(
                RuntimeOrigin::signed(USER_1),
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
            let res = upload_program_default(USER_1, ProgramCodeKind::Default);
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

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let block_author_initial_balance = Balances::free_balance(BLOCK_AUTHOR);
        assert_ok!(upload_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(2, None);

        assert_last_dequeued(1);

        // The block author should be paid the amount of Currency equal to
        // the `gas_charge` incurred while processing the `InitProgram` message
        let gas_spent = GasPrice::gas_price(
            BlockGasLimitOf::<Test>::get()
                .saturating_sub(GasAllowanceOf::<Test>::get())
                .saturating_sub(minimal_weight.ref_time()),
        );
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_author_initial_balance + gas_spent
        );
    })
}

#[test]
fn unused_gas_released_back_works() {
    init_logger();

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let user1_initial_balance = Balances::free_balance(USER_1);
        // This amount is intentionally lower than that hardcoded in the
        // source of ProgramCodeKind::OutgoingWithValueInHandle so the
        // execution ends in a trap sending a message to user's mailbox.
        let huge_send_message_gas_limit = 40_000;

        // Initial value in all gas trees is 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            huge_send_message_gas_limit,
            0
        ));

        // Spends for submit program with default gas limit and sending default message with a huge gas limit
        let user1_potential_msgs_spends =
            GasPrice::gas_price(DEFAULT_GAS_LIMIT + huge_send_message_gas_limit);

        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends
        );
        assert_eq!(
            Balances::reserved_balance(USER_1),
            user1_potential_msgs_spends
        );

        run_to_block(2, None);

        let user1_actual_msgs_spends = GasPrice::gas_price(
            BlockGasLimitOf::<Test>::get()
                .saturating_sub(GasAllowanceOf::<Test>::get())
                .saturating_sub(minimal_weight.ref_time()),
        );

        assert!(user1_potential_msgs_spends > user1_actual_msgs_spends);

        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends
        );

        // All created gas cancels out.
        assert!(GasHandlerOf::<Test>::total_supply().is_zero());
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
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            5_000_000,
            0,
        )
        .expect_err("Must throw err, because code contains start section");
    });
}

#[cfg(feature = "lazy-pages")]
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
    (import "env" "free" (func $free (param i32) (result i32)))
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
            drop
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

        ;; in first run we will access some pages

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
        drop
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
        drop
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
        drop
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
        let res = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .map(|_| prog_id);
        let pid = res.expect("submit result is not ok");

        run_to_block(2, None);
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // First handle: access pages
        let res = Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        );
        assert_ok!(res);

        run_to_block(3, None);
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // Second handle: check pages data
        let res = Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        );
        assert_ok!(res);

        run_to_block(4, None);
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
    });
}

#[cfg(feature = "lazy-pages")]
#[test]
fn lazy_pages() {
    use gear_core::memory::{GearPage, PageU32Size};
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
            ;; store alloc result to 0x0 addr, so 0 page will be already accessed in handle
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
            let res = Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                code,
                salt,
                EMPTY_PAYLOAD.to_vec(),
                10_000_000_000,
                0,
            )
            .map(|_| prog_id);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_block(2, None);
        assert_last_dequeued(1);

        let res = Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            1000,
        );
        assert_ok!(res);

        run_to_block(3, None);

        // Dirty hack: lazy pages info is stored in thread local static variables,
        // so after contract execution lazy-pages information
        // remains correct and we can use it here.
        let write_accessed_pages: BTreeSet<_> = gear_ri::gear_ri::write_accessed_pages()
            .into_iter()
            .collect();

        // checks accessed pages set
        let mut expected_write_accessed_pages = BTreeSet::new();

        // released from 0 wasm page:
        expected_write_accessed_pages.insert(0);

        // released from 2 wasm page:
        expected_write_accessed_pages.insert(0x23ffe / GearPage::size());
        expected_write_accessed_pages.insert(0x24001 / GearPage::size());

        // nothing for 5 wasm page, because it's just read access

        // released from 8 and 9 wasm pages, must be several gear pages:
        expected_write_accessed_pages.insert(0x8fffc / GearPage::size());
        expected_write_accessed_pages.insert(0x90003 / GearPage::size());

        assert_eq!(write_accessed_pages, expected_write_accessed_pages);
    });
}

#[test]
fn initial_pages_cheaper_than_allocated_pages() {
    // When contract has some amount of the initial pages, then it is simpler
    // for core processor and executor than process the same contract
    // but with allocated pages.

    let wat_initial = r#"
    (module
        (import "env" "memory" (memory 0x10))
        (export "init" (func $init))
        (func $init
            (local $i i32)
            ;; make store, so pages are really used
            (loop
                local.get $i
                local.get $i
                i32.store

                local.get $i
                i32.const 0x1000
                i32.add
                local.set $i

                local.get $i
                i32.const 0x100000
                i32.ne
                br_if 0
            )
        )
    )"#;

    let wat_alloc = r#"
    (module
        (import "env" "memory" (memory 0))
        (import "env" "alloc" (func $alloc (param i32) (result i32)))
        (export "init" (func $init))
        (func $init
            (local $i i32)

            ;; alloc 0x100 pages, so mem pages are: 0..=0xff
            (block
                i32.const 0x10
                call $alloc
                i32.eqz
                br_if 0
                unreachable
            )

            ;; make store, so pages are really used
            (loop
                local.get $i
                local.get $i
                i32.store

                local.get $i
                i32.const 0x1000
                i32.add
                local.set $i

                local.get $i
                i32.const 0x100000
                i32.ne
                br_if 0
            )
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let gas_spent = |wat| {
            let res = Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::Custom(wat).to_bytes(),
                DEFAULT_SALT.to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                100_000_000_000,
                0,
            );
            assert_ok!(res);

            run_to_next_block(None);
            assert_last_dequeued(1);

            GasPrice::gas_price(
                BlockGasLimitOf::<Test>::get().saturating_sub(GasAllowanceOf::<Test>::get()),
            )
        };

        let spent_for_initial_pages = gas_spent(wat_initial);
        let spent_for_allocated_pages = gas_spent(wat_alloc);
        assert!(
            spent_for_initial_pages < spent_for_allocated_pages,
            "spent {} gas for initial pages, spent {} gas for allocated pages",
            spent_for_initial_pages,
            spent_for_allocated_pages,
        );
    });
}

#[test]
fn block_gas_limit_works() {
    // Same as `ProgramCodeKind::OutgoingWithValueInHandle`, but without value sending
    let wat1 = r#"
    (module
        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "gr_source" (func $gr_source (param i32)))
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (export "handle_reply" (func $handle_reply))
        (func $handle
            i32.const 111 ;; ptr
            i32.const 1 ;; value
            i32.store

            (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 0) (i32.const 333))

            i32.const 333 ;; addr
            i32.load
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
            local.set $counter
            loop $while
                local.get $counter
                i32.const 1
                i32.add
                local.set $counter
                local.get $counter
                local.get $size
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

    let minimal_weight = mock::get_min_weight();
    let tasks_add_weight = mock::get_weight_of_adding_task();

    new_test_ext().execute_with(|| {
        // =========== BLOCK 2 ============

        // Submit programs and get their ids
        let pid1 = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Custom(wat1));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };
        let pid2 = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Custom(wat2));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // here two programs got initialized
        run_to_next_block(None);
        assert_last_dequeued(2);
        assert_init_success(2);

        let calc_gas = || {
            // Count gas needed to process programs with default payload
            let gas1 = Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(pid1),
                EMPTY_PAYLOAD.to_vec(),
                0,
                true,
            )
            .expect("calculate_gas_info failed");

            // cause pid1 sends messages
            assert!(gas1.burned < gas1.min_limit);

            let gas2 = Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(pid2),
                EMPTY_PAYLOAD.to_vec(),
                0,
                true,
            )
            .expect("calculate_gas_info failed");

            // cause pid2 does nothing except calculations
            assert_eq!(gas2.burned, gas2.min_limit);
            (gas1, gas2)
        };

        // =========== BLOCK 3 ============

        let (gas1, gas2) = calc_gas();

        // showing that min_limit works as expected.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            gas1.min_limit - 1,
            1000
        ));
        let failed1 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            gas1.min_limit,
            1000
        ));
        let succeed1 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            gas2.min_limit - 1,
            1000
        ));
        let failed2 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            gas2.min_limit,
            1000
        ));
        let succeed2 = get_last_message_id();

        run_to_next_block(None);

        assert_last_dequeued(4);
        assert_succeed(succeed1);
        assert_succeed(succeed2);

        assert_failed(
            failed1,
            ActorExecutionErrorReason::Trap(TrapExplanation::GasLimitExceeded),
        );

        assert_failed(
            failed2,
            ActorExecutionErrorReason::Trap(TrapExplanation::GasLimitExceeded),
        );

        // =========== BLOCK 4 ============

        let (gas1, gas2) = calc_gas();

        let send_with_min_limit_to = |pid: ProgramId, gas: &GasInfo| {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                EMPTY_PAYLOAD.to_vec(),
                gas.min_limit,
                1000
            ));
        };

        send_with_min_limit_to(pid1, &gas1);
        send_with_min_limit_to(pid2, &gas2);

        assert!(gas1.burned + gas2.burned < gas1.min_limit + gas2.min_limit);

        // program1 sends message to a user and it goes to the TaskPool
        let weight = minimal_weight + tasks_add_weight;
        // both processed if gas allowance equals only burned count
        run_to_next_block(Some(weight.ref_time() + gas1.burned + gas2.burned));
        assert_last_dequeued(2);

        // =========== BLOCK 5 ============

        send_with_min_limit_to(pid1, &gas1);
        send_with_min_limit_to(pid2, &gas2);
        send_with_min_limit_to(pid1, &gas1);

        // Try to process 3 messages
        run_to_next_block(Some(weight.ref_time() + gas1.burned + gas2.burned - 1));

        // Message #1 is dequeued and processed.
        // Message #2 tried to execute, but exceed gas_allowance is re-queued at the top.
        // Message #3 stays in the queue.
        //
        // | 1 |        | 2 |
        // | 2 |  ===>  | 3 |
        // | 3 |        |   |
        assert_last_dequeued(1);

        // Equals 0 due to trying execution of msg2.
        assert_eq!(GasAllowanceOf::<Test>::get(), 0);

        // =========== BLOCK 6 ============

        // Try to process 2 messages.
        let additional_weight = 12;
        run_to_next_block(Some(
            weight.ref_time() + gas2.burned + gas1.burned + additional_weight,
        ));

        // Both messages got processed.
        //
        // | 2 |        |   |
        // | 3 |  ===>  |   |
        // |   |        |   |

        assert_last_dequeued(2);
        assert_eq!(GasAllowanceOf::<Test>::get(), additional_weight);
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

        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS)
        );

        let (mailbox_message, _bn) = {
            let res = MailboxOf::<Test>::remove(USER_1, reply_to_id);
            assert!(res.is_ok());
            res.expect("was asserted previously")
        };

        assert_eq!(mailbox_message.id(), reply_to_id);

        // Gas limit should have been ignored by the code that puts a message into a mailbox
        assert_eq!(mailbox_message.value(), 1000);

        // Gas is passed into mailboxed messages with reserved value `OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS`
        assert_eq!(
            GasHandlerOf::<Test>::total_supply(),
            OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS
        );
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
                Some(ActorExecutionErrorReason::Trap(
                    TrapExplanation::GasLimitExceeded,
                )),
            ),
        ];

        for (code_kind, trap) in codes {
            System::reset_events();

            assert_ok!(upload_program_default(USER_1, code_kind));

            let event = match System::events().last().map(|r| r.event.clone()) {
                Some(MockRuntimeEvent::Gear(e)) => e,
                _ => unreachable!("Should be one Gear event"),
            };

            run_to_block(next_block, None);

            let msg_id = match event {
                Event::MessageQueued { id, entry, .. } => {
                    if entry == MessageEntry::Init {
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
            let res = upload_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_block(2, None);

        assert!(Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        // Submitting second program, which fails on initialization, therefore is deleted
        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_block(3, None);

        assert!(!Gear::is_initialized(program_id));
        // while at the same time is terminated
        assert!(!Gear::is_active(program_id));
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

        let tests: [(_, _, Option<AssertFailedError>); 4] = [
            // Code, init failure reason, handle succeed flag
            (ProgramCodeKind::Default, None, None),
            (
                ProgramCodeKind::GreedyInit,
                Some(ActorExecutionErrorReason::Trap(
                    TrapExplanation::GasLimitExceeded,
                )),
                Some(SimpleReplyError::NonExecutable.into()),
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_init),
                Some(ActorExecutionErrorReason::Trap(TrapExplanation::Unknown)),
                Some(SimpleReplyError::NonExecutable.into()),
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_handle),
                None,
                Some(ActorExecutionErrorReason::Trap(TrapExplanation::Unknown).into()),
            ),
        ];

        for (code_kind, init_failure_reason, handle_failure_reason) in tests {
            System::reset_events();
            let program_id = {
                let res = upload_program_default(USER_1, code_kind);
                assert_ok!(res);
                res.expect("submit result was asserted")
            };

            let message_id = get_last_message_id();

            System::assert_last_event(
                Event::MessageQueued {
                    id: message_id,
                    source: USER_1,
                    destination: program_id,
                    entry: MessageEntry::Init,
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
                    call_default_message(program_id).dispatch(RuntimeOrigin::signed(USER_1)),
                    Error::<Test>::InactiveProgram
                );

                continue;
            }

            assert_succeed(message_id);

            // Messages to fully-initialized programs are accepted
            assert_ok!(send_default_message(USER_1, program_id));

            let message_id = get_last_message_id();

            System::assert_last_event(
                Event::MessageQueued {
                    id: message_id,
                    source: USER_1,
                    destination: program_id,
                    entry: MessageEntry::Handle,
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
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &USER_1,
            &AccountId::from_origin(prog_id.into_origin()),
            2000,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            1000, // `prog_id` sent message with value of 1000 (see program code)
        ));
        let expected_reply_message_id = get_last_message_id();

        // global nonce is 2 before sending reply message
        // `upload_program` and `send_message` messages were sent before in `setup_mailbox_test_state`
        let event = match System::events().last().map(|r| r.event.clone()) {
            Some(MockRuntimeEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        let actual_reply_message_id = match event {
            Event::MessageQueued {
                id,
                entry: MessageEntry::Reply(_reply_to_id),
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
            Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                MessageId::from_origin(5.into_origin()), // non existent `reply_to_id`
                EMPTY_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                0
            ),
            Error::<Test>::MessageNotFound
        );

        let prog_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        if ProgramStorageOf::<Test>::get_program(prog_id)
            .expect("Failed to get program from storage")
            .0
            .is_terminated()
        {
            panic!("Program is terminated!");
        };

        populate_mailbox_from_program(prog_id, USER_1, 2, 2_000_000_000, 0);

        assert_init_success(1);
        assert_total_dequeued(2);
    })
}

#[test]
fn send_reply_value_claiming_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // This value is actually a constants in WAT. Alternatively can be read from Mailbox.
        let locked_value = 1000;

        // Top up program's account so it could send value in message
        // When program sends message, message value (if not 0) is reserved.
        // If value can't be reserved, message is skipped.
        let send_to_program_amount = locked_value * 2;
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &USER_1,
            &AccountId::from_origin(prog_id.into_origin()),
            send_to_program_amount,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        let mut next_block = 2;

        let user_messages_data = [
            // gas limit, value
            (35_000_000, 4000),
            (45_000_000, 5000),
        ];

        for (gas_limit_to_reply, value_to_reply) in user_messages_data {
            // user 2 triggers program to send message to user 1
            // user 2 after this contains += OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS
            // reserved as MB holding fee
            //
            // here we also run process queue, so on second iteration user 1's
            // first reply got processed and funds freed
            let reply_to_id =
                populate_mailbox_from_program(prog_id, USER_2, next_block, 2_000_000_000, 0);
            next_block += 1;

            let user_balance = Balances::free_balance(USER_1);
            assert_eq!(Balances::reserved_balance(USER_1), 0);

            assert!(MailboxOf::<Test>::contains(&USER_1, &reply_to_id));

            assert_eq!(
                Balances::reserved_balance(USER_2),
                GasPrice::gas_price(OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS)
            );

            // nothing changed
            assert_eq!(Balances::free_balance(USER_1), user_balance);
            assert_eq!(Balances::reserved_balance(USER_1), 0);

            // auto-claim of "locked_value" + send is here
            assert_ok!(Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                reply_to_id,
                EMPTY_PAYLOAD.to_vec(),
                gas_limit_to_reply,
                value_to_reply,
            ));

            let currently_sent = value_to_reply + GasPrice::gas_price(gas_limit_to_reply);

            assert_eq!(
                Balances::free_balance(USER_1),
                user_balance + locked_value - currently_sent
            );
            assert_eq!(Balances::reserved_balance(USER_1), currently_sent);
            assert_eq!(Balances::reserved_balance(USER_2), 0,);
        }
    })
}

// user 1 sends to prog msg
// prog send to user 1 msg to mailbox
// user 1 claims it from mailbox
#[test]
fn claim_value_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let sender_balance = Balances::free_balance(USER_2);
        assert_eq!(Balances::reserved_balance(USER_2), 0);
        let claimer_balance = Balances::free_balance(USER_1);
        assert_eq!(Balances::reserved_balance(USER_1), 0);

        let gas_sent = 10_000_000_000;
        let value_sent = 1000;

        let prog_id = {
            let res = upload_program_default(USER_3, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        increase_prog_balance_for_mailbox_test(USER_3, prog_id);

        let reply_to_id = populate_mailbox_from_program(prog_id, USER_2, 2, gas_sent, value_sent);
        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        let bn_of_insertion = Gear::block_number();
        let holding_duration = 4;

        let GasInfo {
            burned: gas_burned,
            may_be_returned,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        let gas_burned = GasPrice::gas_price(gas_burned - may_be_returned);

        run_to_block(bn_of_insertion + holding_duration, None);

        let block_producer_balance = Balances::free_balance(BLOCK_AUTHOR);

        assert_ok!(Gear::claim_value(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
        ));

        assert_eq!(Balances::reserved_balance(USER_1), 0);
        assert_eq!(Balances::reserved_balance(USER_2), 0);

        let expected_claimer_balance = claimer_balance + value_sent;
        assert_eq!(Balances::free_balance(USER_1), expected_claimer_balance);

        let burned_for_hold =
            GasPrice::gas_price(holding_duration * CostsPerBlockOf::<Test>::mailbox());

        // In `calculate_gas_info` program start to work with page data in storage,
        // so need to take in account gas, which spent for data loading.
        let charged_for_page_load = if cfg!(feature = "lazy-pages") {
            GasPrice::gas_price(
                <Test as Config>::Schedule::get()
                    .memory_weights
                    .load_page_data
                    .ref_time(),
            )
        } else {
            0
        };

        // Gas left returns to sender from consuming of value tree while claiming.
        let expected_sender_balance =
            sender_balance + charged_for_page_load - value_sent - gas_burned - burned_for_hold;
        assert_eq!(Balances::free_balance(USER_2), expected_sender_balance);
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_producer_balance + burned_for_hold
        );

        System::assert_last_event(
            Event::UserMessageRead {
                id: reply_to_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            }
            .into(),
        );
    })
}

#[test]
fn uninitialized_program_zero_gas() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            50_000_000_000u64,
            0u128
        ));

        let init_message_id = utils::get_last_message_id();
        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_block(2, None);

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));
        assert!(WaitlistOf::<Test>::contains(&program_id, &init_message_id));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(1),
            program_id,
            vec![],
            0, // that triggers unreachable code atm
            0,
        ));

        run_to_block(3, None);
    })
}

#[test]
fn distributor_initialize() {
    use demo_distributor::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = Balances::free_balance(USER_1) + Balances::free_balance(BLOCK_AUTHOR);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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
        let final_balance = Balances::free_balance(USER_1) + Balances::free_balance(BLOCK_AUTHOR);

        assert_eq!(initial_balance, final_balance);
    });
}

#[test]
fn distributor_distribute() {
    use demo_distributor::{Request, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = Balances::free_balance(USER_1) + Balances::free_balance(BLOCK_AUTHOR);

        // Initial value in all gas trees is 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_id = generate_program_id(WASM_BINARY, DEFAULT_SALT);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            3_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            Request::Receive(10).encode(),
            10_000_000_000,
            0,
        ));

        run_to_block(3, None);

        // We sent two messages in mailbox
        let mail_box_len = 2;
        assert_eq!(MailboxOf::<Test>::len(&USER_1), mail_box_len);

        // Despite some messages are still in the mailbox all gas locked in value trees
        // has been refunded to the sender so the free balances should add up
        let final_balance = Balances::free_balance(USER_1) + Balances::free_balance(BLOCK_AUTHOR);

        let mailbox_threshold_gas_limit =
            mail_box_len as u64 * <Test as Config>::MailboxThreshold::get();
        let mailbox_threshold_reserved =
            <Test as Config>::GasPrice::gas_price(mailbox_threshold_gas_limit);
        assert_eq!(initial_balance - mailbox_threshold_reserved, final_balance);

        // All gas cancelled out in the end
        assert_eq!(
            GasHandlerOf::<Test>::total_supply(),
            mailbox_threshold_gas_limit
        );
    });
}

#[test]
fn test_code_submission_pass() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_hash = generate_code_hash(&code).into();
        let code_id = CodeId::from_origin(code_hash);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone()
        ));

        let saved_code = <Test as Config>::CodeStorage::get_code(code_id);

        let schedule = <Test as Config>::Schedule::get();
        let code = Code::try_new(
            code,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
        )
        .expect("Error creating Code");
        assert_eq!(saved_code.unwrap().code(), code.code());

        let expected_meta = Some(common::CodeMetadata::new(USER_1.into_origin(), 1));
        let actual_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
        assert_eq!(expected_meta, actual_meta);

        // TODO: replace this temporary (`None`) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        System::assert_last_event(
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

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone()
        ),);
        // Trying to set the same code twice.
        assert_noop!(
            Gear::upload_code(RuntimeOrigin::signed(USER_1), code.clone()),
            Error::<Test>::CodeAlreadyExists,
        );
        // Trying the same from another origin
        assert_noop!(
            Gear::upload_code(RuntimeOrigin::signed(USER_2), code),
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
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));

        // TODO: replace this temporary (`None`) value
        // for expiration block number with properly
        // calculated one (issues #646 and #969).
        System::assert_has_event(
            Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            }
            .into(),
        );
        assert!(<Test as Config>::CodeStorage::exists(code_id));

        // Trying to set the same code twice.
        assert_noop!(
            Gear::upload_code(RuntimeOrigin::signed(USER_2), code),
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
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone()
        ));
        let expected_code_saved_events = 1;
        let expected_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
        assert!(expected_meta.is_some());

        // Submit program from another origin. Should not change meta or code.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        let actual_meta = <Test as Config>::CodeStorage::get_metadata(code_id);
        let actual_code_saved_events = System::events()
            .iter()
            .filter(|e| {
                matches!(
                    e.event,
                    MockRuntimeEvent::Gear(Event::CodeChanged {
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

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            50_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_block(2, None);

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(1),
            program_id,
            vec![],
            10_000u64,
            0u128
        ));

        run_to_block(3, None);

        assert_eq!(
            ProgramStorageOf::<Test>::waiting_init_take_messages(program_id).len(),
            1
        );
    })
}

#[test]
fn uninitialized_program_should_accept_replies() {
    use demo_init_wait::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            10_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_block(2, None);

        // there should be one message for the program author
        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            10_000_000_000u64,
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

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            10_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            10_000_000_000u64,
            0,
        ));

        run_to_block(3, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            10_000_000_000u64,
            0u128
        ));

        run_to_block(4, None);

        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);
        assert_eq!(
            MailboxOf::<Test>::iter_key(USER_1)
                .next()
                .map(|(msg, _bn)| msg.payload().to_vec())
                .expect("Element should be"),
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

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            Vec::new(),
            10_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        // While program is not inited all messages addressed to it are waiting.
        // There could be dozens of them.
        let n = 10;
        for _ in 0..n {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_3),
                program_id,
                vec![],
                5_000_000_000u64,
                0u128
            ));
        }

        run_to_block(3, None);

        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            20_000_000_000u64,
            0,
        ));

        run_to_block(20, None);

        let actual_n = MailboxOf::<Test>::iter_key(USER_3).fold(0usize, |i, (m, _bn)| {
            assert_eq!(m.payload().to_vec(), b"Hello, world!".encode());
            i + 1
        });

        assert_eq!(actual_n, n);
    })
}

#[test]
fn test_different_waits_success() {
    use demo_waiter::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_active(program_id));

        let reserve_gas = CostsPerBlockOf::<Test>::reserve_for()
            .saturated_into::<u64>()
            .saturating_mul(CostsPerBlockOf::<Test>::waitlist());

        let duration_gas = |duration: u32| {
            duration
                .saturated_into::<u64>()
                .saturating_mul(CostsPerBlockOf::<Test>::waitlist())
        };

        let expiration = |duration: u32| -> BlockNumberFor<Test> {
            Gear::block_number().saturating_add(duration.unique_saturated_into())
        };

        let system_reservation = demo_waiter::system_reserve();

        // Command::Wait case.
        let payload = Command::Wait.encode();
        let duration = 5;
        let wl_gas = duration_gas(duration) + reserve_gas;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_success = get_last_message_id();

        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_success), expiration(duration));

        // Command::WaitFor case.
        let duration = 5;
        let payload = Command::WaitFor(duration).encode();
        let wl_gas = duration_gas(duration) + reserve_gas + 100_000_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_for_success = get_last_message_id();

        run_to_next_block(None);

        assert_eq!(
            get_waitlist_expiration(wait_for_success),
            expiration(duration)
        );

        // Command::WaitUpTo case.
        let duration = 5;
        let payload = Command::WaitUpTo(duration).encode();
        let wl_gas = duration_gas(duration) + reserve_gas + 100_000_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_up_to_success = get_last_message_id();

        run_to_next_block(None);

        assert_eq!(
            get_waitlist_expiration(wait_up_to_success),
            expiration(duration)
        );
    });
}

#[test]
fn test_different_waits_fail() {
    use demo_waiter::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_active(program_id));

        let system_reservation = demo_waiter::system_reserve();

        // Command::Wait case no gas.
        let payload = Command::Wait.encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_gas,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Wait(
                WaitError::NotEnoughGas,
            ))),
        );

        // Command::WaitFor case no gas.
        let payload = Command::WaitFor(10).encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_for_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_for_gas,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Wait(
                WaitError::NotEnoughGas,
            ))),
        );

        // Command::WaitUpTo case no gas.
        let payload = Command::WaitUpTo(10).encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_up_to_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_up_to_gas,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Wait(
                WaitError::NotEnoughGas,
            ))),
        );

        // Command::WaitFor case invalid argument.
        let payload = Command::WaitFor(0).encode();
        let wl_gas = 10_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            // Hack to avoid calculating gas info fail.
            Command::WaitFor(1).encode(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_for_arg = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_for_arg,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Wait(
                WaitError::InvalidArgument,
            ))),
        );

        // Command::WaitUpTo case invalid argument.
        let payload = Command::WaitUpTo(0).encode();
        let wl_gas = 10_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            // Hack to avoid calculating gas info fail.
            Command::WaitUpTo(1).encode(),
            value,
            false,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value
        ));

        let wait_up_to_arg = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_up_to_arg,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Wait(
                WaitError::InvalidArgument,
            ))),
        );
    });
}

// TODO:
//
// introduce new tests for this in #1485
#[test]
fn test_requeue_after_wait_for_timeout() {
    use demo_waiter::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        let duration = 10;
        let payload = Command::SendAndWaitFor(duration, USER_1.into()).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            10_000_000_000,
            0,
        ));

        // Fast forward blocks.
        let message_id = get_last_message_id();
        run_to_next_block(None);
        let now = System::block_number();

        System::set_block_number(duration as u64 + now - 1);
        Gear::set_block_number((duration as u64 + now - 1).try_into().unwrap());

        // Clean previous events and mailbox.
        System::reset_events();
        MailboxOf::<Test>::clear();
        run_to_next_block(None);

        // `MessageWoken` dispatched.
        System::assert_has_event(MockRuntimeEvent::Gear(Event::MessageWoken {
            id: message_id,
            reason: Reason::Runtime(MessageWokenRuntimeReason::WakeCalled),
        }));

        // Message waited again.
        System::assert_has_event(MockRuntimeEvent::Gear(Event::MessageWaited {
            id: message_id,
            origin: None,
            reason: Reason::Runtime(MessageWaitedRuntimeReason::WaitForCalled),
            expiration: 23,
        }));

        // Message processed.
        assert_eq!(get_last_mail(USER_1).payload(), b"ping");
    })
}

#[test]
fn test_sending_waits() {
    use demo_waiter::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // utils
        let expiration = |duration: u32| -> BlockNumberFor<Test> {
            System::block_number().saturating_add(duration.unique_saturated_into())
        };

        // upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            20_000_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();

        // Check that block number matches program upload block number
        let upload_block_number = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), upload_block_number);

        run_to_next_block(None);

        // Case 1 - `Command::SendFor`
        //
        // Send message and then wait_for.
        let duration = 5;
        let payload = Command::SendFor(USER_1.into(), duration).encode();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            25_000_000_000,
            0,
        ));

        // Check that block number was changed after first message sent
        let block_number_after_send = System::block_number();
        assert_eq!(Gear::get_block_number(program_id), block_number_after_send);

        let wait_for = get_last_message_id();
        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_for), expiration(duration));

        // Case 2 - `Command::SendUpTo`
        //
        // Send message and then wait_up_to.
        let duration = 10;
        let payload = Command::SendUpTo(USER_1.into(), duration).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            25_000_000_000,
            0,
        ));

        let wait_no_more = get_last_message_id();
        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_no_more), expiration(duration));

        // Case 3 - `Command::SendUpToWait`
        //
        // Send message and then wait no_more, wake, wait no_more again.
        let duration = 10;
        let payload = Command::SendUpToWait(USER_2.into(), duration).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
        ));

        let wait_wait = get_last_message_id();
        run_to_next_block(None);
        assert_eq!(get_waitlist_expiration(wait_wait), expiration(duration));

        let reply_to_id = MailboxOf::<Test>::iter_key(USER_2)
            .next()
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");

        // wake `wait_wait`
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_2),
            reply_to_id,
            vec![],
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);

        // Check that block number was not changed, 'cause program state not changed
        assert_eq!(Gear::get_block_number(program_id), block_number_after_send);

        assert_eq!(
            get_waitlist_expiration(wait_wait),
            expiration(demo_waiter::default_wait_up_to_duration())
        );
    });
}

#[test]
fn test_wait_timeout() {
    use demo_wait_timeout::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // `Command::SendTimeout`
        //
        // Emits error when locks are timeout
        let duration = 10;
        let payload = Command::SendTimeout(USER_1.into(), duration).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);
        let now = System::block_number();
        let target = duration as u64 + now - 1;

        // Try waking the processed message.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            Command::Wake.encode(),
            10_000_000,
            0,
        ));

        run_to_next_block(None);
        System::set_block_number(target);
        Gear::set_block_number(target.try_into().unwrap());
        System::reset_events();
        run_to_next_block(None);

        // Timeout still works.
        assert!(MailboxOf::<Test>::iter_key(USER_1)
            .any(|(msg, _bn)| msg.payload().to_vec() == b"timeout"));
    })
}

#[test]
fn test_join_wait_timeout() {
    use demo_wait_timeout::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // Join two waited messages, futures complete at
        // the same time when both of them are finished.
        let duration_a = 5;
        let duration_b = 10;
        let payload = Command::JoinTimeout(USER_1.into(), duration_a, duration_b).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);

        // Run to each of the targets and check if we can get the timeout result.
        let now = System::block_number();
        let targets = [duration_a, duration_b].map(|target| target as u64 + now - 1);
        let run_to_target = |target: u64| {
            System::set_block_number(target);
            Gear::set_block_number(target.try_into().unwrap());
            run_to_next_block(None);
        };

        // Run to the end of the first duration.
        //
        // The timeout message has not been triggered yet.
        run_to_target(targets[0]);
        assert!(!MailboxOf::<Test>::iter_key(USER_1)
            .any(|(msg, _bn)| msg.payload().to_vec() == b"timeout"));

        // Run to the end of the second duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[1]);
        assert!(MailboxOf::<Test>::iter_key(USER_1)
            .any(|(msg, _bn)| msg.payload().to_vec() == b"timeout"));
    })
}

#[test]
fn test_select_wait_timeout() {
    use demo_wait_timeout::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // Select from two waited messages, futures complete at
        // the same time when one of them getting failed.
        let duration_a = 5;
        let duration_b = 10;
        let payload = Command::SelectTimeout(USER_1.into(), duration_a, duration_b).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);

        // Run to the end of the first duration.
        //
        // The timeout message has been triggered.
        let now = System::block_number();
        let target = duration_a as u64 + now - 1;
        System::set_block_number(target);
        Gear::set_block_number(target.try_into().unwrap());
        run_to_next_block(None);

        assert!(MailboxOf::<Test>::iter_key(USER_1)
            .any(|(msg, _bn)| msg.payload().to_vec() == b"timeout"));
    })
}

#[test]
fn test_wait_lost() {
    use demo_wait_timeout::{Command, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000u64,
            0u128
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        let duration_a = 5;
        let duration_b = 10;
        let payload = Command::WaitLost(USER_1.into()).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);

        assert!(MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| {
            if msg.payload() == b"ping" {
                assert_ok!(Gear::send_reply(
                    RuntimeOrigin::signed(USER_1),
                    msg.id(),
                    b"ping".to_vec(),
                    100_000_000,
                    0
                ));

                true
            } else {
                false
            }
        }));

        let now = System::block_number();
        let targets = [duration_a, duration_b].map(|target| target as u64 + now - 1);
        let run_to_target = |target: u64| {
            System::set_block_number(target);
            Gear::set_block_number(target.try_into().unwrap());
            run_to_next_block(None);
        };

        // Run to the end of the first duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[0]);
        assert!(
            !MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload() == b"unreachable")
        );

        // Run to the end of the second duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[1]);
        assert!(MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload() == b"timeout"));
        assert!(MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload() == b"timeout2"));
        assert!(MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload() == b"success"));
    })
}

#[test]
fn test_message_processing_for_non_existing_destination() {
    init_logger();
    new_test_ext().execute_with(|| {
        let program_id =
            upload_program_default(USER_1, ProgramCodeKind::GreedyInit).expect("Failed to init");
        let code_hash =
            generate_code_hash(ProgramCodeKind::GreedyInit.to_bytes().as_slice()).into();
        let user_balance_before = Balances::free_balance(USER_1);

        // After running, first message will end up with init failure, so destination address won't exist.
        // However, message to that non existing address will be in message queue. So, we test that this message is not executed.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000,
            1_000
        ));

        let skipped_message_id = get_last_message_id();
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        run_to_block(2, None);

        assert_not_executed(skipped_message_id);

        // some funds may be unreserved after processing init-message
        assert!(user_balance_before <= Balances::free_balance(USER_1));

        assert!(!Gear::is_active(program_id));
        assert!(<Test as Config>::CodeStorage::exists(code_hash));
    })
}

#[test]
fn exit_locking_funds() {
    use demo_exit_handle_sender::{Input, WASM_BINARY as EXIT_HANDLE_SENDER_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            EXIT_HANDLE_SENDER_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            50_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        let user_2_balance = Balances::free_balance(USER_2);

        assert!(Gear::is_initialized(program_id));

        assert_balance(program_id, 0u128, 0u128);

        let value = 1_000;

        let payload = Input::SendMessage {
            destination: program_id.into_origin().into(),
            payload: vec![],
            value,
        };
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload.encode(),
            1_000_000_000,
            value
        ));
        let message_1 = utils::get_last_message_id();

        let payload = Input::Exit(USER_2.into_origin().into());
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload.encode(),
            1_000_000_000,
            0
        ));
        let message_2 = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(message_1);
        assert_succeed(message_2);

        assert_balance(USER_2, user_2_balance + value, 0u128);
        assert_balance(program_id, 0u128, 0u128);
    });
}

#[test]
fn terminated_locking_funds() {
    use demo_init_fail_sender::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let GasInfo {
            min_limit: gas_spent_init,
            waited: init_waited,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(WASM_BINARY.to_vec()),
            USER_3.into_origin().encode(),
            5_000,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(init_waited);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
        ));

        let schedule = Schedule::<Test>::default();
        let code_id = get_last_code_id();
        let code = <Test as Config>::CodeStorage::get_code(code_id)
            .expect("code should be in the storage");
        let code_length = code.code().len();
        let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
        let module_instantiation =
            schedule.module_instantiation_per_byte.ref_time() * code_length as u64;
        let system_reservation = demo_init_fail_sender::system_reserve();
        let reply_duration = demo_init_fail_sender::reply_duration();
        let gas_for_code_len = read_cost;

        // Value which must be returned to `USER1` after init message processing complete.
        let prog_free = 4000u128;
        // Reserved value, which is sent to user in init and then we wait for reply from user.
        let prog_reserve = 1000u128;

        let locked_gas_to_wl = CostsPerBlockOf::<Test>::waitlist()
            * (reply_duration as u64 + CostsPerBlockOf::<Test>::reserve_for());
        let gas_spent_in_wl = CostsPerBlockOf::<Test>::waitlist();
        // Value, which will be returned to init message after wake.
        let returned_from_wait_list =
            <Test as Config>::GasPrice::gas_price(locked_gas_to_wl - gas_spent_in_wl);

        // Value, which will be returned to `USER1` after init message processing complete.
        let returned_from_system_reservation =
            <Test as Config>::GasPrice::gas_price(system_reservation);

        // Additional gas for loading resources on next wake up.
        // Must be exactly equal to gas, which we must pre-charge for program execution.
        let gas_for_second_init_execution = core_processor::calculate_gas_for_program(read_cost, 0)
            + gas_for_code_len
            + core_processor::calculate_gas_for_code(
                read_cost,
                <Test as Config>::Schedule::get()
                    .db_read_per_byte
                    .ref_time(),
                code_length as u64,
            )
            + module_instantiation
            + <Test as Config>::Schedule::get()
                .memory_weights
                .static_page
                .ref_time()
                * code.static_pages().raw() as u64;

        // Because we set gas for init message second execution only for resources loading, then
        // after execution system reserved gas and sended value and price for wait list must be returned
        // to user. This is because contract will stop his execution on first wasm block, because of gas
        // limit exceeded. So, gas counter will be equal to amount of returned from wait list gas in handle reply.
        let expected_balance_difference =
            prog_free + returned_from_wait_list + returned_from_system_reservation;

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            DEFAULT_SALT.to_vec(),
            USER_3.into_origin().encode(),
            gas_spent_init + gas_for_second_init_execution,
            5_000u128
        ));

        let program_id = get_last_program_id();
        let message_id = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_active(program_id));
        assert_balance(program_id, prog_free, prog_reserve);

        let (_message_with_value, interval) = MailboxOf::<Test>::iter_key(USER_3)
            .next()
            .map(|(msg, interval)| (msg.id(), interval))
            .expect("Element should be");

        let message_to_reply = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .map(|(msg, _)| msg.id())
            .expect("Element should be");

        let GasInfo {
            min_limit: gas_spent_reply,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Reply(message_to_reply, 0),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_to_reply,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent_reply,
            0
        ));

        let reply_id = get_last_message_id();

        let user_1_balance = Balances::free_balance(USER_1);

        run_to_next_block(None);

        assert_succeed(reply_id);
        assert_failed(
            message_id,
            ActorExecutionErrorReason::Trap(TrapExplanation::GasLimitExceeded),
        );
        assert!(Gear::is_terminated(program_id));
        assert_balance(program_id, 0u128, prog_reserve);

        let expected_balance = user_1_balance + expected_balance_difference;
        let user_1_balance = Balances::free_balance(USER_1);

        assert_eq!(user_1_balance, expected_balance);

        // Hack to fast spend blocks till expiration.
        System::set_block_number(interval.finish - 1);
        Gear::set_block_number((interval.finish - 1).try_into().unwrap());

        run_to_next_block(None);

        assert!(MailboxOf::<Test>::is_empty(&USER_3));

        let extra_gas_to_mb = <Test as Config>::GasPrice::gas_price(
            CostsPerBlockOf::<Test>::mailbox() * CostsPerBlockOf::<Test>::reserve_for(),
        );

        let expected_balance = user_1_balance + prog_reserve + extra_gas_to_mb;

        assert_balance(program_id, 0u128, 0u128);
        assert_eq!(Balances::free_balance(USER_1), expected_balance);
    });
}

#[test]
fn exit_init() {
    use demo_exit_init::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        let code_id = CodeId::generate(WASM_BINARY);
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            vec![],
            [0].to_vec(),
            50_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert!(!Gear::is_active(program_id));
        assert!(!Gear::is_initialized(program_id));
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        // Program is not removed and can't be submitted again
        assert_noop!(
            Gear::create_program(
                RuntimeOrigin::signed(USER_1),
                code_id,
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
fn test_create_program_works() {
    use demo_init_wait::WASM_BINARY;

    init_logger();

    new_test_ext().execute_with(|| {
        System::reset_events();

        let code = WASM_BINARY.to_vec();
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone(),
        ));

        // Parse wasm code.
        let schedule = <Test as Config>::Schedule::get();
        let code = Code::try_new(
            code,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
        )
        .expect("Code failed to load");

        let code_id = CodeId::generate(code.raw_code());
        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            vec![],
            Vec::new(),
            // # TODO
            //
            // Calculate the gas spent after #1242.
            10_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(Gear::is_active(program_id));

        run_to_next_block(None);

        // there should be one message for the program author
        let message_id = MailboxOf::<Test>::iter_key(USER_1)
            .next()
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            b"PONG".to_vec(),
            // # TODO
            //
            // Calculate the gas spent after #1242.
            10_000_000_000u64,
            0,
        ));

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));
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
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        ));

        // Try to create a program with non existing code hash
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            50_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Init and dispatch messages from the contract are dequeued, but not executed
        // 2 error replies are generated, and executed (forwarded to USER_2 mailbox).
        assert_eq!(MailboxOf::<Test>::len(&USER_2), 2);
        assert_total_dequeued(4 + 2); // +2 for upload_program/send_messages
        assert_init_success(1); // 1 for submitting factory

        System::reset_events();
        MailboxOf::<Test>::clear();

        // Try to create multiple programs with non existing code hash
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (valid_code_hash, b"salt1".to_vec(), 5_000_000_000),
                (valid_code_hash, b"salt2".to_vec(), 5_000_000_000),
                (valid_code_hash, b"salt3".to_vec(), 5_000_000_000),
            ])
            .encode(),
            100_000_000_000,
            0,
        ));
        run_to_block(3, None);

        assert_eq!(MailboxOf::<Test>::len(&USER_2), 6);
        assert_total_dequeued(12 + 1);
        assert_init_success(0);

        assert_noop!(
            Gear::upload_code(
                RuntimeOrigin::signed(USER_1),
                invalid_prog_code_kind.to_bytes(),
            ),
            Error::<Test>::ProgramConstructionFailed,
        );

        System::reset_events();
        MailboxOf::<Test>::clear();

        // Try to create with invalid code hash
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (invalid_prog_code_hash, b"salt1".to_vec(), 5_000_000_000),
                (invalid_prog_code_hash, b"salt2".to_vec(), 5_000_000_000),
                (invalid_prog_code_hash, b"salt3".to_vec(), 5_000_000_000),
            ])
            .encode(),
            100_000_000_000,
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
        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), child_code,));

        // Creating factory
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Test create one successful in init program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            50_000_000_000,
            0,
        ));
        run_to_block(3, None);

        // Test create one failing in init program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(
                vec![(child_code_hash, b"some_data".to_vec(), 300_000)] // too little gas
            )
            .encode(),
            10_000_000_000,
            0,
        ));
        run_to_block(4, None);

        // First extrinsic call with successful program creation dequeues and executes init and dispatch messages
        // Second extrinsic is failing one, for each message it generates replies, which are executed (4 dequeued, 2 dispatched)
        assert_total_dequeued(6 + 3); // +3 for extrinsics
        assert_init_success(1 + 1); // +1 for submitting factory

        System::reset_events();

        // Create multiple successful init programs
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt1".to_vec(), 200_000_000),
                (child_code_hash, b"salt2".to_vec(), 200_000_000),
            ])
            .encode(),
            50_000_000_000,
            0,
        ));
        run_to_block(5, None);

        // Create multiple successful init programs
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt3".to_vec(), 300_000), // too little gas
                (child_code_hash, b"salt4".to_vec(), 300_000), // too little gas
            ])
            .encode(),
            50_000_000_000,
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
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            child_code.clone(),
        ));

        // Creating factory
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            20_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // User creates a program
        assert_ok!(upload_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(3, None);

        // Program tries to create the same
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
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
        assert_total_dequeued(3 + 3); // +3 from extrinsics (2 upload_program, 1 send_message)
        assert_init_success(2); // +2 from extrinsics (2 upload_program)

        System::reset_events();
        MailboxOf::<Test>::clear();

        // Create a new program from program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 2_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
        ));
        run_to_block(5, None);

        // Try to create the same
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
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
            Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
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

        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_2), child_code,));

        // Creating factory
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            2_000_000_000,
            0,
        ));
        run_to_block(2, None);

        // Try to create duplicate during one execution
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt1".to_vec(), 1_000_000_000), // could be successful init
                (child_code_hash, b"salt1".to_vec(), 1_000_000_000), // duplicate
            ])
            .encode(),
            20_000_000_000,
            0,
        ));

        run_to_block(3, None);

        // Duplicate init fails the call and returns error reply to the caller, which is USER_1.
        // State roll-back is performed.
        assert_total_dequeued(2); // 2 for extrinsics
        assert_init_success(1); // 1 for creating a factory

        System::reset_events();

        // Successful child creation
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 1_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
        ));

        run_to_block(4, None);

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

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_2),
            child1_code,
        ));
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_2),
            child2_code,
        ));

        // Creating factory
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            factory_code.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child1_code_hash, b"salt1".to_vec(), 200_000_000),
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child1_code_hash, b"salt2".to_vec(), 100_000),
            ])
            .encode(),
            50_000_000_000,
            0,
        ));

        run_to_block(3, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 300_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt2".to_vec(), 200_000_000),
            ])
            .encode(),
            50_000_000_000,
            0,
        ));

        run_to_block(4, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![
                // duplicate in the next block: init not executed, nor the handle (because destination is terminated), replies are generated (+4 dequeue, +2 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 200_000_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt3".to_vec(), 200_000_000),
            ])
            .encode(),
            50_000_000_000,
            0,
        ));

        run_to_block(5, None);

        assert_total_dequeued(18 + 4); // +4 for 3 send_message calls and 1 upload_program call
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
        let code_id = CodeId::generate(WASM_BINARY);
        let code_hash = generate_code_hash(&code).into();
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            vec![],
            Vec::new(),
            10_000_000_000u64,
            0u128
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(program_id));

        // An expensive operation since "gr_exit" removes all program pages from storage.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            50_000_000_000u64,
            0u128
        ));

        run_to_block(3, None);

        assert!(!Gear::is_active(program_id));
        assert!(MailboxOf::<Test>::is_empty(&USER_3));
        assert!(!Gear::is_initialized(program_id));
        assert!(!Gear::is_active(program_id));

        assert!(<Test as Config>::CodeStorage::exists(CodeId::from_origin(
            code_hash
        )));

        // Program is not removed and can't be submitted again
        assert_noop!(
            Gear::create_program(
                RuntimeOrigin::signed(USER_1),
                code_id,
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
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
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
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog_id,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent,
            0,
        ));

        let msg_id = get_last_message_id();
        assert_ok!(GasHandlerOf::<Test>::get_limit(msg_id), gas_spent);

        // before execution
        let free_after_send = Balances::free_balance(USER_1);
        let reserved_after_send = Balances::reserved_balance(USER_1);
        assert_eq!(reserved_after_send, GasPrice::gas_price(gas_spent));

        run_to_block(3, None);

        // gas_limit has been recovered
        assert_noop!(
            GasHandlerOf::<Test>::get_limit(msg_id),
            pallet_gear_gas::Error::<Test>::NodeNotFound
        );

        // the (reserved_after_send - gas_spent) has been unreserved
        let free_after_execution = Balances::free_balance(USER_1);
        assert_eq!(
            free_after_execution,
            free_after_send + (reserved_after_send - GasPrice::gas_price(gas_spent))
        );

        // reserved balance after execution is zero
        let reserved_after_execution = Balances::reserved_balance(USER_1);
        assert!(reserved_after_execution.is_zero());
    })
}

#[test]
fn init_wait_reply_exit_cleaned_storage() {
    use demo_init_wait_reply_exit::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            Vec::new(),
            50_000_000_000u64,
            0u128
        ));
        let pid = get_last_program_id();

        // block 2
        //
        // - send messages to the program
        run_to_block(2, None);
        let count = 5;
        for _ in 0..count {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
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
            .map(|(msg, _bn)| msg.id())
            .expect("Element should be");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg_id,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000_000u64,
            0,
        ));

        assert!(!Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        // block 4
        //
        // - check if program has terminated
        // - check waiting_init storage is empty
        // - check wait list is empty
        run_to_block(4, None);
        assert!(!Gear::is_initialized(pid));
        assert!(!Gear::is_active(pid));
        assert_eq!(waiting_init_messages(pid).len(), 0);
        assert_eq!(WaitlistOf::<Test>::iter_key(pid).count(), 0);
    })
}

#[test]
fn locking_gas_for_waitlist() {
    use demo_gas_burned::WASM_BINARY as GAS_BURNED_BINARY;
    use demo_gasless_wasting::{InputArgs, WASM_BINARY as GASLESS_WASTING_BINARY};

    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_wait" (func $gr_wait))
        (export "handle" (func $handle))
        (func $handle call $gr_wait)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        // This program just waits on each handle message.
        let waiter = upload_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("submit result was asserted");

        // This program just does some calculations (burns gas) on each handle message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            GAS_BURNED_BINARY.to_vec(),
            Default::default(),
            Default::default(),
            100_000_000_000,
            0
        ));
        let calculator = get_last_program_id();

        // This program sends two empty gasless messages on each handle:
        // for this test first message is waiter, seconds is calculator.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            GASLESS_WASTING_BINARY.to_vec(),
            Default::default(),
            Default::default(),
            DEFAULT_GAS_LIMIT,
            0
        ));
        let sender = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(waiter));
        assert!(Gear::is_initialized(calculator));
        assert!(Gear::is_initialized(sender));

        let payload = InputArgs {
            prog_to_wait: waiter.into_origin().into(),
            prog_to_waste: calculator.into_origin().into(),
        };

        calculate_handle_and_send_with_extra(USER_1, sender, payload.encode(), None, 0);
        let origin_msg_id = get_last_message_id();

        let message_to_be_waited = MessageId::generate_outgoing(origin_msg_id, 1);

        run_to_block(3, None);

        assert!(WaitlistOf::<Test>::contains(&waiter, &message_to_be_waited));

        let mut expiration = None;

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                id,
                expiration: exp,
                ..
            }) = e.event
            {
                if id == message_to_be_waited {
                    expiration = Some(exp);
                }
            }
        });

        let expiration = expiration.unwrap();

        // Expiration block may be really far from current one, so proper
        // `run_to_block` takes a lot, so we use hack here by setting
        // close block number to it to check that messages keeps in
        // waitlist before and leaves it as expected.
        System::set_block_number(expiration - 2);
        Gear::set_block_number((expiration - 2).try_into().unwrap());

        run_to_next_block(None);

        assert!(WaitlistOf::<Test>::contains(&waiter, &message_to_be_waited));

        run_to_next_block(None);

        // And nothing panics here, because `message_to_be_waited`
        // contains enough founds to pay rent.

        assert!(!WaitlistOf::<Test>::contains(
            &waiter,
            &message_to_be_waited
        ));
    });
}

#[test]
fn calculate_init_gas() {
    use demo_gas_burned::WASM_BINARY;

    init_logger();
    let gas_info_1 = new_test_ext().execute_with(|| {
        Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(WASM_BINARY.to_vec()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap()
    });

    let gas_info_2 = new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec()
        ));

        let code_id = get_last_code_id();

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::InitByHash(code_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap();

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            gas_info.min_limit,
            0
        ));

        let init_message_id = get_last_message_id();

        run_to_next_block(None);

        assert_succeed(init_message_id);

        gas_info
    });

    assert_eq!(gas_info_1, gas_info_2);
}

#[test]
fn gas_spent_vs_balance() {
    use demo_btree::{Request, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = Balances::free_balance(USER_1);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        ));

        let prog_id = utils::get_last_program_id();

        run_to_block(2, None);

        let balance_after_init = Balances::free_balance(USER_1);

        let request = Request::Clear.encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog_id,
            request.clone(),
            1_000_000_000,
            0
        ));

        run_to_block(3, None);

        let balance_after_handle = Balances::free_balance(USER_1);
        let total_balance_after_handle = Balances::total_balance(&USER_1);

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
        .unwrap();

        // check that all changes made by calculate_gas_info are rollbacked
        assert_eq!(balance_after_handle, Balances::free_balance(USER_1));
        assert_eq!(total_balance_after_handle, Balances::total_balance(&USER_1));

        assert_eq!(
            (initial_balance - balance_after_init),
            GasPrice::gas_price(init_gas_spent)
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
        .unwrap();

        assert_eq!(
            balance_after_init - balance_after_handle,
            GasPrice::gas_price(handle_gas_spent)
        );
    });
}

#[test]
fn gas_spent_precalculated() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (func $add (; 0 ;) (param $0 i32) (param $1 i32)
            (local $2 i32)
            local.get $0
            local.get $1
            i32.add
            local.set $2
        )
        (func $handle
            (call $add
                (i32.const 2)
                (i32.const 2)
            )
        )
    )"#;

    let wat_no_counter = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init)
    )"#;

    let wat_init = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init
            (local $1 i32)
            i32.const 1
            local.set $1
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let prog = ProgramCodeKind::Custom(wat);
        let prog_id = upload_program_default(USER_1, prog).expect("submit result was asserted");

        let init_gas_id = upload_program_default(USER_3, ProgramCodeKind::Custom(wat_init))
            .expect("submit result was asserted");
        let init_no_counter_id =
            upload_program_default(USER_3, ProgramCodeKind::Custom(wat_no_counter))
                .expect("submit result was asserted");

        run_to_block(2, None);

        let code_id = CodeId::generate(&prog.to_bytes());
        let code = <Test as Config>::CodeStorage::get_code(code_id).unwrap();
        let code = code.code();

        let init_gas_code_id = CodeId::from_origin(ProgramStorageOf::<Test>::get_program(init_gas_id)
            .and_then(|(p, _bn)| common::ActiveProgram::try_from(p).ok())
            .expect("program must exist")
            .code_hash);
        let init_code_len: u64 = <Test as Config>::CodeStorage::get_code(init_gas_code_id).unwrap().code().len() as u64;

        let init_no_gas_code_id = CodeId::from_origin(ProgramStorageOf::<Test>::get_program(init_no_counter_id)
            .and_then(|(p, _bn)| common::ActiveProgram::try_from(p).ok())
            .expect("program must exist")
            .code_hash);
        let init_no_gas_code_len: u64 = <Test as Config>::CodeStorage::get_code(init_no_gas_code_id).unwrap().code().len() as u64;

        // binaries have the same memory amount but different lengths
        // so take this into account in gas calculations
        let length_margin = init_code_len - init_no_gas_code_len;

        let GasInfo {
            min_limit: gas_spent_init,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(ProgramCodeKind::Custom(wat_init).to_bytes()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap();

        let GasInfo {
            min_limit: gas_spent_no_counter,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(ProgramCodeKind::Custom(wat_no_counter).to_bytes()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap();

        let schedule = <Test as Config>::Schedule::get();
        let per_byte_cost = schedule.db_read_per_byte.ref_time();
        let const_i64_cost = schedule.instruction_weights.i64const;
        let set_local_cost = schedule.instruction_weights.local_set;
        let module_instantiation_per_byte = schedule.module_instantiation_per_byte.ref_time();

        // gas_charge call in handle and "add" func
        let gas_cost = gas_spent_init
            - gas_spent_no_counter
            - const_i64_cost as u64
            - set_local_cost as u64
            - core_processor::calculate_gas_for_code(0, per_byte_cost, length_margin)
            - module_instantiation_per_byte * length_margin;

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
        .unwrap();

        let call_cost = schedule.instruction_weights.call;
        let get_local_cost = schedule.instruction_weights.local_get;
        let add_cost = schedule.instruction_weights.i32add;
        let module_instantiation = module_instantiation_per_byte * code.len() as u64;

        let total_cost = {
            let cost = call_cost
                + const_i64_cost * 2
                + set_local_cost
                + get_local_cost * 2
                + add_cost
                + gas_cost as u32 * 2;

            let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();

            u64::from(cost)
                // cost for loading program
                + core_processor::calculate_gas_for_program(read_cost, 0)
                // cost for loading code length
                + read_cost
                // cost for loading code
                + core_processor::calculate_gas_for_code(read_cost, per_byte_cost, code.len() as u64)
                + module_instantiation
                // cost for one static page in program
                + <Test as Config>::Schedule::get().memory_weights.static_page.ref_time()
        };

        assert_eq!(gas_spent_1, total_cost);

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
        let contract_code_id = CodeId::generate(MUL_CONST_WASM_BINARY);
        let compose_id = generate_program_id(COMPOSE_WASM_BINARY, b"salt");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"contract_a".to_vec(),
            50_u64.encode(),
            10_000_000_000,
            0,
        ));

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            contract_code_id,
            b"contract_b".to_vec(),
            75_u64.encode(),
            10_000_000_000,
            0,
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            COMPOSE_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            (
                <[u8; 32]>::from(contract_a_id),
                <[u8; 32]>::from(contract_b_id)
            )
                .encode(),
            10_000_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            compose_id,
            100_u64.to_le_bytes().to_vec(),
            30_000_000_000,
            0,
        ));

        run_to_block(4, None);

        // Gas total issuance should have gone back to 4 * MAILBOX_THRESHOLD
        assert_eq!(
            GasHandlerOf::<Test>::total_supply(),
            <Test as Config>::MailboxThreshold::get() * 4
        );
    });
}

// Before introducing this test, upload_program extrinsic didn't check the value.
// Also value wasn't check in `create_program` sys-call. There could be the next test case, which could affect badly.
//
// User submits program with value X, which is not checked. Say X < ED. If we send handle and reply messages with
// values during the init message processing, internal checks will result in errors (either, because sending value
// Y <= X < ED is not allowed, or because of Y > X, when X < ED).
// However, in this same situation of program being initialized and sending some message with value, if program send
// init message with value Y <= X < ED, no internal checks will occur, so such message sending will be passed further
// to manager, although having value less than ED.
//
// Note: on manager level message will not be included to the queue.
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
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        // Can't initialize program with value less than ED
        assert_noop!(
            Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::Default.to_bytes(),
                b"test0".to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                100_000_000,
                ed - 1,
            ),
            Error::<Test>::ValueLessThanMinimal,
        );

        // Simple passing test with values
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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

        System::reset_events();

        // Trying to send init message from program with value less than ED.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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
        // to send init message with value in invalid range.
        assert_total_dequeued(1);

        assert_failed(
            msg_id,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Message(
                MessageError::InsufficientValue {
                    message_value: 499,
                    existential_deposit: 500,
                },
            ))),
        );
    })
}

// Before introducing this test, upload_program extrinsic didn't check the value.
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
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        let sending_to_program = 2 * get_ed();
        let random_receiver = 1;
        // Trying to send init message from program with value greater than program can send.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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
        // to send init message with value more than program has.
        assert_total_dequeued(1);

        assert_failed(
            origin_msg_id,
            ActorExecutionErrorReason::Trap(TrapExplanation::Ext(ExtError::Message(
                MessageError::NotEnoughValue {
                    message_value: 1001,
                    value_left: 1000,
                },
            ))),
        );
    })
}

#[test]
fn test_create_program_without_gas_works() {
    use demo_init_with_value::{SendMessage, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
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
        let reply_call = crate::mock::RuntimeCall::Gear(crate::Call::<Test>::send_reply {
            reply_to_id: mail_id,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: 10_000_000,
            value: 0,
        });
        assert_noop!(
            reply_call.dispatch(RuntimeOrigin::signed(USER_1)),
            Error::<Test>::InactiveProgram,
        );

        // the only way to claim value from terminated destination is a corresponding extrinsic call
        assert_ok!(Gear::claim_value(RuntimeOrigin::signed(USER_1), mail_id,));

        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        System::assert_last_event(
            Event::UserMessageRead {
                id: mail_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            }
            .into(),
        )
    })
}

#[test]
fn calculate_gas_info_for_wait_dispatch_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Test should still be valid once #1173 solved.
        let GasInfo { waited, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(demo_init_wait::WASM_BINARY.to_vec()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .unwrap();

        assert!(waited);
    });
}

#[test]
fn delayed_sending() {
    use demo_delayed_sender::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let delay = 3u32;
        // Deploy program, which sends mail in "payload" amount of blocks.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            delay.to_le_bytes().to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0
        ));

        let prog = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_active(prog));

        for _ in 0..delay {
            assert!(maybe_last_message(USER_1).is_none());
            run_to_next_block(None);
        }

        assert_eq!(get_last_mail(USER_1).payload(), b"Delayed hello!");
    });
}

#[test]
fn delayed_wake() {
    use demo_delayed_sender::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0u32.to_le_bytes().to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0
        ));

        let prog = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_active(prog));

        assert!(maybe_last_message(USER_1).is_some());

        // This message will go into waitlist.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog,
            vec![],
            BlockGasLimitOf::<Test>::get(),
            0
        ));

        let mid = get_last_message_id();

        assert!(!WaitlistOf::<Test>::contains(&prog, &mid));

        run_to_next_block(None);

        assert!(WaitlistOf::<Test>::contains(&prog, &mid));

        let delay = 3u32;

        // This message will wake previous message in "payload" blocks
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog,
            delay.to_le_bytes().to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0
        ));

        run_to_next_block(None);

        for _ in 0..delay {
            assert!(WaitlistOf::<Test>::contains(&prog, &mid));
            run_to_next_block(None);
        }

        assert!(!WaitlistOf::<Test>::contains(&prog, &mid));
    });
}

#[test]
fn cascading_messages_with_value_do_not_overcharge() {
    init_logger();
    new_test_ext().execute_with(|| {
        let contract_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract");
        let wrapper_id = generate_program_id(WAITING_PROXY_WASM_BINARY, b"salt");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"contract".to_vec(),
            50_u64.encode(),
            5_000_000_000,
            0,
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITING_PROXY_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            <[u8; 32]>::from(contract_id).encode(),
            5_000_000_000,
            0,
        ));

        run_to_block(2, None);

        let payload = 100_u64.to_le_bytes().to_vec();

        let user_balance_before_calculating = Balances::free_balance(USER_1);

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

        let user_initial_balance = Balances::free_balance(USER_1);

        let mailbox_threshold_reserved =
            <Test as Config>::GasPrice::gas_price(<Test as Config>::MailboxThreshold::get());

        assert_eq!(user_balance_before_calculating, user_initial_balance);
        assert_eq!(
            Balances::reserved_balance(USER_1),
            mailbox_threshold_reserved * 2
        );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            wrapper_id,
            payload,
            gas_reserved,
            value,
        ));

        let gas_to_spend = GasPrice::gas_price(gas_to_spend);
        let gas_reserved = GasPrice::gas_price(gas_reserved);
        let reserved_balance = gas_reserved + value;

        assert_eq!(
            Balances::free_balance(USER_1),
            user_initial_balance - reserved_balance
        );

        assert_eq!(
            Balances::reserved_balance(USER_1),
            reserved_balance + mailbox_threshold_reserved * 2
        );

        run_to_block(5, None);

        assert_eq!(
            Balances::reserved_balance(USER_1),
            mailbox_threshold_reserved * 3
        );

        assert_eq!(
            Balances::free_balance(USER_1),
            user_initial_balance - gas_to_spend - value - mailbox_threshold_reserved
        );
    });
}

#[test]
fn free_storage_hold_on_scheduler_overwhelm() {
    use demo_value_sender::{TestData, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
        ));

        let sender = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(sender));

        let data = TestData::gasful(20_000, 0);

        let mb_cost = CostsPerBlockOf::<Test>::mailbox();
        let reserve_for = CostsPerBlockOf::<Test>::reserve_for();

        let user_1_balance = Balances::free_balance(USER_1);
        assert_eq!(Balances::reserved_balance(USER_1), 0);

        let user_2_balance = Balances::free_balance(USER_2);
        assert_eq!(Balances::reserved_balance(USER_2), 0);

        let prog_balance = Balances::free_balance(AccountId::from_origin(sender.into_origin()));
        assert_eq!(
            Balances::reserved_balance(AccountId::from_origin(sender.into_origin())),
            0
        );

        let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
            USER_1,
            sender,
            data.request(USER_2).encode(),
            Some(data.extra_gas),
            0,
        );

        utils::assert_balance(
            USER_1,
            user_1_balance - GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
            GasPrice::gas_price(gas_info.min_limit + data.extra_gas),
        );
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        run_to_next_block(None);

        let hold_bound = HoldBound::<Test>::by(CostsPerBlockOf::<Test>::mailbox())
            .maximum_for(data.gas_limit_to_send);

        let expected_duration = data.gas_limit_to_send / mb_cost - reserve_for;

        assert_eq!(
            hold_bound.expected_duration(),
            expected_duration.saturated_into::<BlockNumberFor<Test>>()
        );

        utils::assert_balance(
            USER_1,
            user_1_balance - GasPrice::gas_price(gas_info.burned + data.gas_limit_to_send),
            GasPrice::gas_price(data.gas_limit_to_send),
        );
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance - data.value, data.value);
        assert!(!MailboxOf::<Test>::is_empty(&USER_2));

        // Expected block.
        run_to_block(hold_bound.expected(), Some(0));
        assert!(!MailboxOf::<Test>::is_empty(&USER_2));

        // Deadline block (can pay till this one).
        run_to_block(hold_bound.deadline(), Some(0));
        assert!(!MailboxOf::<Test>::is_empty(&USER_2));

        // Block which already can't be payed.
        run_to_next_block(None);

        let gas_totally_burned = GasPrice::gas_price(gas_info.burned + data.gas_limit_to_send);

        utils::assert_balance(USER_1, user_1_balance - gas_totally_burned, 0u128);
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));
    });
}

#[test]
fn execution_over_blocks() {
    init_logger();

    let assert_last_message = |src: [u8; 32], count: u128| {
        use demo_calc_hash::verify_result;

        let last_message = maybe_last_message(USER_1).expect("Get last message failed.");
        let result = <[u8; 32]>::decode(&mut last_message.payload()).expect("Decode result failed");

        assert!(verify_result(src, count, result));

        System::reset_events();
    };

    let estimate_gas_per_calc = || -> (u64, u64) {
        use demo_calc_hash_in_one_block::{Package, WASM_BINARY};

        let (src, times) = ([0; 32], 1);

        let init_gas = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(WASM_BINARY.to_vec()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("Failed to get gas spent");

        // deploy demo-calc-in-one-block
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"estimate threshold".to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            init_gas.burned,
            0,
        ));
        let in_one_block = get_last_program_id();

        run_to_next_block(None);

        // estimate start cost
        let pkg = Package::new(times, src);
        let gas = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(in_one_block),
            pkg.encode(),
            0,
            true,
        )
        .expect("Failed to get gas spent");

        (init_gas.min_limit, gas.min_limit)
    };

    new_test_ext().execute_with(|| {
        use demo_calc_hash_in_one_block::{Package, WASM_BINARY};

        // We suppose that gas limit is less than gas allowance
        let block_gas_limit = BlockGasLimitOf::<Test>::get() - 10000;

        // Deploy demo-calc-hash-in-one-block.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            5_000_000_000,
            0,
        ));
        let in_one_block = get_last_program_id();

        assert!(ProgramStorageOf::<Test>::program_exists(in_one_block));

        let src = [0; 32];

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            in_one_block,
            Package::new(128, src).encode(),
            block_gas_limit,
            0,
        ));

        run_to_next_block(None);

        assert_last_message([0; 32], 128);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            in_one_block,
            Package::new(17_384, src).encode(),
            block_gas_limit,
            0,
        ));

        let message_id = get_last_message_id();
        run_to_next_block(None);

        assert_failed(
            message_id,
            ActorExecutionErrorReason::Trap(TrapExplanation::GasLimitExceeded),
        );
    });

    new_test_ext().execute_with(|| {
        use demo_calc_hash::sha2_512_256;
        use demo_calc_hash_over_blocks::{Method, WASM_BINARY};
        let block_gas_limit = BlockGasLimitOf::<Test>::get();

        let (_, calc_threshold) = estimate_gas_per_calc();

        // deploy demo-calc-hash-over-blocks
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            calc_threshold.encode(),
            10_000_000_000,
            0,
        ));
        let over_blocks = get_last_program_id();

        assert!(ProgramStorageOf::<Test>::program_exists(over_blocks));

        let (src, id, expected) = ([0; 32], sha2_512_256(b"42"), 8_192);

        // trigger calculation
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            over_blocks,
            Method::Start { src, id, expected }.encode(),
            10_000_000_000,
            0,
        ));

        run_to_next_block(None);

        let mut count = 0;
        while maybe_last_message(USER_1).is_none() {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                over_blocks,
                Method::Refuel(id).encode(),
                block_gas_limit,
                0,
            ));

            count += 1;
            run_to_next_block(None);
        }

        assert!(count > 1);
        assert_last_message(src, expected);
    });
}

#[test]
fn call_forbidden_function() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_gas_available" (func $gr_gas_available (param i32)))
        (export "handle" (func $handle))
        (func $handle
            i32.const 0
            call $gr_gas_available
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let prog_id = upload_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("submit result was asserted");

        run_to_block(2, None);

        let res = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        );

        assert_eq!(
            res,
            Err(format!(
                "Program terminated with a trap: {}",
                TrapExplanation::ForbiddenFunction,
            ))
        );
    });
}

#[test]
fn test_async_messages() {
    use demo_async_tester::{Kind, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        System::reset_events();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
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
                RuntimeOrigin::signed(USER_1),
                pid,
                kind.encode(),
                10_000_000_000u64,
                0,
            ));

            // check the message sent from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(Kind::decode(&mut last_mail.payload()), Ok(*kind));

            // reply to the message
            let message_id = last_mail.id();
            assert_ok!(Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                message_id,
                EMPTY_PAYLOAD.to_vec(),
                10_000_000_000u64,
                0,
            ));

            // check the reply from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(last_mail.payload(), b"PONG");
            assert_ok!(Gear::claim_value(
                RuntimeOrigin::signed(USER_1),
                last_mail.id()
            ));
        }

        assert!(Gear::is_active(pid));
    })
}

#[test]
fn missing_functions_are_not_executed() {
    // handle is copied from ProgramCodeKind::OutgoingWithValueInHandle
    let wat = r#"
    (module
        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
        (import "env" "memory" (memory 10))
        (export "handle" (func $handle))
        (func $handle
            i32.const 111 ;; addr
            i32.const 1 ;; value
            i32.store

            i32.const 143 ;; addr + 32
            i32.const 1000
            i32.store

            (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 0) (i32.const 333))

            i32.const 333 ;; addr
            i32.load
            (if
                (then unreachable)
                (else)
            )
        )
    )"#;

    init_logger();

    new_test_ext().execute_with(|| {
        let balance_before = Balances::free_balance(USER_1);

        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Custom(wat));
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(ProgramCodeKind::Custom(wat).to_bytes()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        let program_cost = core_processor::calculate_gas_for_program(
            DbWeightOf::<Test>::get().reads(1).ref_time(),
            <Test as Config>::Schedule::get()
                .db_read_per_byte
                .ref_time(),
        );
        // there is no execution so the values should be equal
        assert_eq!(min_limit, program_cost);

        run_to_next_block(None);

        // there is no 'init' so memory pages and code don't get loaded and
        // no execution is performed at all and hence user was not charged for program execution.
        assert_eq!(
            balance_before,
            Balances::free_balance(USER_1) + GasPrice::gas_price(program_cost)
        );

        // this value is actually a constant in the wat.
        let locked_value = 1_000;
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &USER_1,
            &AccountId::from_origin(program_id.into_origin()),
            locked_value,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
        ));

        run_to_next_block(None);

        let reply_to_id = get_last_mail(USER_1).id();

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Reply(reply_to_id, 0),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_eq!(min_limit, program_cost);

        let balance_before = Balances::free_balance(USER_1);
        let reply_value = 1_500;
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            reply_value,
        ));

        run_to_next_block(None);

        assert_eq!(
            balance_before - reply_value + locked_value,
            Balances::free_balance(USER_1) + GasPrice::gas_price(program_cost)
        );
    });
}

#[test]
fn missing_handle_is_not_executed() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 2))
        (export "init" (func $init))
        (func $init)
    )"#;

    let wat_handle = r#"
    (module
        (import "env" "memory" (memory 2))
        (export "init" (func $init))
        (export "handle" (func $handle))
        (func $init)
        (func $handle)
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat).to_bytes(),
            vec![],
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .expect("submit_program failed");

        let program_handle_id = Gear::upload_program(
            RuntimeOrigin::signed(USER_3),
            ProgramCodeKind::Custom(wat_handle).to_bytes(),
            vec![],
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .expect("submit_program failed");

        run_to_next_block(None);

        let balance_before = Balances::free_balance(USER_1);
        let balance_before_handle = Balances::free_balance(USER_3);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            program_handle_id,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
        ));

        run_to_next_block(None);

        let margin = balance_before - Balances::free_balance(USER_1);
        let margin_handle = balance_before_handle - Balances::free_balance(USER_3);

        assert!(margin < margin_handle);
    });
}

#[test]
fn invalid_memory_page_count_rejected() {
    let wat = format!(
        r#"
    (module
        (import "env" "memory" (memory {}))
        (export "init" (func $init))
        (func $init)
    )"#,
        code::MAX_WASM_PAGE_COUNT + 1
    );

    init_logger();
    new_test_ext().execute_with(|| {
        assert_noop!(
            Gear::upload_code(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::Custom(&wat).to_bytes(),
            ),
            Error::<Test>::ProgramConstructionFailed
        );

        assert_noop!(
            Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::Custom(&wat).to_bytes(),
                vec![],
                EMPTY_PAYLOAD.to_vec(),
                1_000_000_000,
                0,
            ),
            Error::<Test>::ProgramConstructionFailed
        );
    });
}

#[test]
fn test_mad_big_prog_instrumentation() {
    init_logger();
    new_test_ext().execute_with(|| {
        let path = "../../examples/big-wasm/big.wasm";
        let code_bytes = std::fs::read(path).expect("can't read big wasm");
        let schedule = <Test as Config>::Schedule::get();
        let code_inst_res = gear_core::code::Code::try_new(
            code_bytes,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
        );
        // In any case of the defined weights on the platform, instrumentation of the valid
        // huge wasm mustn't fail
        assert!(code_inst_res.is_ok());
    })
}

#[test]
fn reject_incorrect_binary() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (func $handle
            i32.const 5
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_noop!(
            Gear::upload_code(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::CustomInvalid(wat).to_bytes()
            ),
            Error::<Test>::ProgramConstructionFailed
        );

        assert_noop!(
            upload_program_default(USER_1, ProgramCodeKind::CustomInvalid(wat)),
            Error::<Test>::ProgramConstructionFailed
        );
    });
}

#[test]
fn send_from_reservation() {
    use demo_send_from_reservation::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        let pid2 = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![2],
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        run_to_block(2, None);

        {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::SendToUser.encode(),
                10_000_000_000,
                1_000,
            ));

            run_to_block(3, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 500);
            assert_eq!(msg.payload(), b"send_to_user");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::SendToProgram {
                    pid: pid2.into(),
                    user: USER_1.into_origin().into()
                }
                .encode(),
                10_000_000_000,
                1_000,
            ));

            let mid = get_last_message_id();

            run_to_block(4, None);

            assert_succeed(mid);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 700);
            assert_eq!(msg.payload(), b"receive_from_program");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::SendToUserDelayed.encode(),
                10_000_000_000,
                1_000,
            ));

            run_to_block(5, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));

            run_to_block(6, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 600);
            assert_eq!(msg.payload(), b"send_to_user_delayed");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::SendToProgramDelayed {
                    pid: pid2.into(),
                    user: USER_1.into_origin().into()
                }
                .encode(),
                10_000_000_000,
                1_000,
            ));

            let mid = get_last_message_id();

            run_to_block(7, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));
            assert_succeed(mid);

            run_to_block(8, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 800);
            assert_eq!(msg.payload(), b"receive_from_program_delayed");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }
    });
}

#[test]
fn reply_from_reservation() {
    use demo_send_from_reservation::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![],
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        let pid2 = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            vec![2],
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        run_to_block(2, None);

        {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::ReplyToUser.encode(),
                10_000_000_000,
                1_000,
            ));

            run_to_block(3, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 900);
            assert_eq!(msg.payload(), b"reply_to_user");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::ReplyToProgram {
                    pid: pid2.into(),
                    user: USER_1.into_origin().into()
                }
                .encode(),
                10_000_000_000,
                1_000,
            ));

            let mid = get_last_message_id();

            run_to_block(4, None);

            assert_succeed(mid);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 900);
            assert_eq!(msg.payload(), b"reply");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::ReplyToUserDelayed.encode(),
                10_000_000_000,
                1_000,
            ));

            run_to_block(5, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));

            run_to_block(6, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 1000);
            assert_eq!(msg.payload(), b"reply_to_user_delayed");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }

        {
            MailboxOf::<Test>::clear();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::ReplyToProgramDelayed {
                    pid: pid2.into(),
                    user: USER_1.into_origin().into()
                }
                .encode(),
                10_000_000_000,
                1_000,
            ));

            let mid = get_last_message_id();

            run_to_block(7, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));
            assert_succeed(mid);

            run_to_block(8, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 1000);
            assert_eq!(msg.payload(), b"reply_delayed");
            let map = get_reservation_map(pid).unwrap();
            assert!(map.is_empty());
        }
    });
}

#[test]
fn signal_recursion_not_occurs() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::PanicInSignal.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        let mut expiration = None;

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                expiration: exp, ..
            }) = e.event
            {
                expiration = Some(exp);
            }
        });

        let expiration = expiration.unwrap();

        System::set_block_number(expiration - 1);
        Gear::set_block_number((expiration - 1).try_into().unwrap());

        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch panicked
        assert_eq!(MailboxOf::<Test>::iter_key(USER_1).last(), None);
        let signal_msg_id = MessageId::generate_signal(mid);
        let status = dispatch_status(signal_msg_id);
        assert_eq!(status, Some(DispatchStatus::Failed));

        MailboxOf::<Test>::clear();
        System::reset_events();
        run_to_next_block(None);

        // check nothing happens after
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(System::events().len(), 0);
    });
}

#[test]
fn signal_during_precharge() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitWithReserveAmountAndPanic(1).encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let reply_to_id = get_last_mail(USER_1).id();

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(
            System::events()
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.event,
                        MockRuntimeEvent::Gear(Event::UserMessageSent { .. })
                    )
                })
                .count(),
            2 // reply from program + reply to user because of panic
        );
    });
}

#[test]
fn signal_during_prepare() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
        let schedule = <Test as Config>::Schedule::get();
        let program_gas = core_processor::calculate_gas_for_program(
            read_cost,
            schedule.db_read_per_byte.ref_time(),
        );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitWithReserveAmountAndPanic(program_gas).encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let reply_to_id = get_last_mail(USER_1).id();

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(
            System::events()
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.event,
                        MockRuntimeEvent::Gear(Event::UserMessageSent { .. })
                    )
                })
                .count(),
            2 // reply from program + reply to user because of panic
        );
    });
}

#[test]
fn signal_async_wait_works() {
    use demo_async_signal_entry::{InitAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::None.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        let GasInfo {
            min_limit: gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent,
            0,
        ));

        let mid = get_last_message_id();

        let mut expiration = None;

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                expiration: exp, ..
            }) = e.event
            {
                expiration = Some(exp);
            }
        });

        let expiration = expiration.unwrap();

        System::set_block_number(expiration - 1);
        Gear::set_block_number((expiration - 1).try_into().unwrap());

        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload(), b"handle_signal");
    });
}

#[test]
fn signal_gas_limit_exceeded_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::OutOfGas.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload(), b"handle_signal");
    });
}

#[test]
fn system_reservation_unreserve_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let user_initial_balance = Balances::free_balance(USER_1);

        let GasInfo { burned, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            HandleAction::Simple.encode(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Simple.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        let burned = GasPrice::gas_price(burned);
        assert_eq!(
            Balances::free_balance(USER_1),
            user_initial_balance - burned
        );
    });
}

#[test]
fn few_system_reservations_across_waits_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::AcrossWaits.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);
        let mut reserved = GasHandlerOf::<Test>::get_system_reserve(mid).unwrap();

        for _ in 0..5 {
            assert_eq!(GasHandlerOf::<Test>::get_system_reserve(mid), Ok(reserved));
            reserved += 1_000_000_000;

            let reply_to_id = get_last_mail(USER_1).id();
            assert_ok!(Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                reply_to_id,
                EMPTY_PAYLOAD.to_vec(),
                10_000_000_000,
                0
            ));

            run_to_next_block(None);
        }
    });
}

#[test]
fn system_reservation_panic_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Panic.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload(), b"handle_signal");
    });
}

#[test]
fn system_reservation_exit_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Exit.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_succeed(mid);
        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch was not executed but `gr_exit` did
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);
        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload(), b"exit");
    });
}

#[test]
fn system_reservation_wait_and_panic_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndPanic.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let reply_to_id = get_last_mail(USER_1).id();

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());
    });
}

#[test]
fn system_reservation_wait_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Wait.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        let mut expiration = None;

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                expiration: exp, ..
            }) = e.event
            {
                expiration = Some(exp);
            }
        });

        let expiration = expiration.unwrap();

        System::set_block_number(expiration - 1);
        Gear::set_block_number((expiration - 1).try_into().unwrap());

        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());
    });
}

#[test]
fn system_reservation_wait_and_exit_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndExit.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let reply_to_id = get_last_mail(USER_1).id();

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check `gr_exit` occurs
        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload(), b"wait_and_exit");
    });
}

#[test]
fn system_reservation_wait_and_reserve_with_panic_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndReserveWithPanic.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_eq!(
            GasHandlerOf::<Test>::get_system_reserve(mid),
            Ok(2_000_000_000)
        );

        let reply_to_id = get_last_mail(USER_1).id();
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload(), b"handle_signal");
    });
}

#[test]
fn system_reservation_accumulate_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Accumulate.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let reserve = GasHandlerOf::<Test>::get_system_reserve(mid).unwrap();
        // we 1000 and then 234 amount of gas in demo
        assert_eq!(reserve, 1234);
    });
}

#[test]
fn system_reservation_zero_amount_panics() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::ZeroReserve.encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_succeed(mid);
    });
}

#[test]
fn gas_reservation_works() {
    use demo_reserve_gas::{HandleAction, InitAction, RESERVATION_AMOUNT};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::Normal.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        // gas has been reserved 3 times
        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 3);

        let user_initial_balance = Balances::free_balance(USER_1);

        let GasInfo {
            min_limit: spent_gas,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            HandleAction::Unreserve.encode(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Unreserve.encode(),
            spent_gas,
            0
        ));

        run_to_block(3, None);

        // gas unreserved manually
        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 2);

        let gas_reserved = GasPrice::gas_price(spent_gas);
        let reservation_amount = GasPrice::gas_price(RESERVATION_AMOUNT);
        let reservation_holding = 5 * GasPrice::gas_price(CostsPerBlockOf::<Test>::reservation());

        assert_eq!(
            Balances::free_balance(USER_1),
            user_initial_balance - gas_reserved + reservation_amount + reservation_holding
        );

        run_to_block(2 + 2, None);

        // gas not yet unreserved automatically
        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 2);

        run_to_block(2 + 3, None);

        // gas unreserved automatically
        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 1);

        // check task is exist yet
        let (reservation_id, slot) = map.iter().next().unwrap();
        let task = ScheduledTask::RemoveGasReservation(pid, *reservation_id);
        assert!(TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));

        // `gr_exit` occurs
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Exit.encode(),
            50_000_000_000,
            0
        ));

        run_to_block(2 + 4, None);

        // check task was cleared after `gr_exit` happened
        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));
    });
}

#[test]
fn gas_reservations_cleaned_in_terminated_program() {
    use demo_reserve_gas::{InitAction, ReplyAction};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::Wait.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let message_id = get_last_mail(USER_1).id();

        assert!(!Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 1);

        let (reservation_id, slot) = map.iter().next().unwrap();
        let task = ScheduledTask::RemoveGasReservation(pid, *reservation_id);
        assert!(TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            ReplyAction::Panic.encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));

        run_to_block(3, None);

        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));
        assert!(!Gear::is_initialized(pid));
        assert!(!Gear::is_active(pid));
    });
}

#[test]
fn gas_reservation_wait_wake_exit() {
    use demo_reserve_gas::{InitAction, ReplyAction};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::Wait.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let message_id = get_last_mail(USER_1).id();

        assert!(!Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 1);

        let (reservation_id, slot) = map.iter().next().unwrap();
        let task = ScheduledTask::RemoveGasReservation(pid, *reservation_id);
        assert!(TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            ReplyAction::Exit.encode(),
            DEFAULT_GAS_LIMIT * 10,
            0,
        ));

        run_to_block(3, None);

        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &BlockNumberFor::<Test>::from(slot.finish),
            &task
        ));
        assert!(!Gear::is_initialized(pid));
        assert!(!Gear::is_active(pid));
    });
}

#[test]
fn gas_reservations_check_params() {
    use demo_reserve_gas::InitAction;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::CheckArgs {
                mailbox_threshold: <Test as Config>::MailboxThreshold::get(),
            }
            .encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(2, None);

        assert_succeed(mid);
    });
}

#[test]
fn custom_async_entrypoint_works() {
    use demo_async_custom_entry::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        ));

        run_to_block(3, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload(), b"my_handle_signal");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0
        ));

        run_to_block(4, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload(), b"my_handle_reply");
    });
}

#[test]
fn dispatch_kind_forbidden_function() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(Gear::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::ForbiddenCallInSignal(USER_1.into_origin().into()).encode(),
            10_000_000_000,
            0,
        ));

        let mid = get_last_message_id();

        let mut expiration = None;

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                expiration: exp, ..
            }) = e.event
            {
                expiration = Some(exp);
            }
        });

        let expiration = expiration.unwrap();

        System::set_block_number(expiration - 1);
        Gear::set_block_number((expiration - 1).try_into().unwrap());

        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch panicked
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        let signal_msg_id = MessageId::generate_signal(mid);
        let status = dispatch_status(signal_msg_id);
        assert_eq!(status, Some(DispatchStatus::Failed));

        MailboxOf::<Test>::clear();
        System::reset_events();
        run_to_next_block(None);

        // check nothing happens after
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(System::events().len(), 0);
    });
}

#[test]
fn system_reservation_gas_allowance_rollbacks() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            HandleAction::Simple.encode(),
            0,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Simple.encode(),
            min_limit,
            0,
        ));

        let mid = get_last_message_id();

        run_to_block(3, Some(min_limit - 1));

        assert_eq!(GasHandlerOf::<Test>::get_system_reserve(mid), Ok(0));
    });
}

#[test]
fn system_reservation_wait_and_exit_across_executions() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Wait.encode(),
            10_000_000_000,
            0,
        ));

        let mid_wait = get_last_message_id();

        run_to_block(3, None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid_wait));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Exit.encode(),
            10_000_000_000,
            0,
        ));

        let mid_exit = get_last_message_id();

        run_to_block(4, None);

        assert!(Gear::is_exited(pid));
        assert!(GasHandlerOf::<Test>::get_system_reserve(mid_wait).is_err());
        assert!(GasHandlerOf::<Test>::get_system_reserve(mid_exit).is_err());

        MailboxOf::<Test>::clear();

        let mut expiration = None;

        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessageWaited {
                expiration: exp, ..
            }) = e.event
            {
                expiration = Some(exp);
            }
        });

        let expiration = expiration.unwrap();

        System::set_block_number(expiration - 1);
        Gear::set_block_number((expiration - 1).try_into().unwrap());

        run_to_next_block(None);

        // nothing happened after
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
    });
}

#[test]
fn signal_on_uninitialized_program() {
    use demo_async_signal_entry::{InitAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::Panic.encode(),
            10_000_000_000,
            0,
        ));

        let pid = get_last_program_id();
        let init_mid = get_last_message_id();

        run_to_block(2, None);

        assert!(Gear::is_active(pid));
        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(init_mid));

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload(), b"init");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
        ));

        let reply_mid = get_last_message_id();

        run_to_block(3, None);

        assert!(!Gear::is_initialized(pid));
        assert!(GasHandlerOf::<Test>::get_system_reserve(init_mid).is_err());
        assert!(GasHandlerOf::<Test>::get_system_reserve(reply_mid).is_err());
    });
}

mod utils {
    #![allow(unused)]

    use super::{
        assert_ok, pallet, run_to_block, Event, MailboxOf, MockRuntimeEvent, RuntimeOrigin, Test,
    };
    use crate::{
        mock::{Balances, Gear, System},
        BalanceOf, GasInfo, HandleKind, ProgramStorageOf, SentOf,
    };
    use common::{
        event::*,
        storage::{CountedByKey, Counter, IterableByKeyMap},
        Origin, ProgramStorage,
    };
    use core::fmt::Display;
    use core_processor::common::ActorExecutionErrorReason;
    use frame_support::{
        codec::Decode,
        dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
        traits::tokens::{currency::Currency, Balance},
    };
    use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
    use gear_backend_common::TrapExplanation;
    use gear_core::{
        ids::{CodeId, MessageId, ProgramId},
        message::{Message, Payload, ReplyDetails, StatusCode, StoredMessage},
        reservation::GasReservationMap,
    };
    use gear_core_errors::{ExtError, SimpleCodec, SimpleReplyError};
    use sp_core::H256;
    use sp_runtime::traits::UniqueSaturatedInto;
    use sp_std::{convert::TryFrom, fmt::Debug};

    pub(super) const DEFAULT_GAS_LIMIT: u64 = 200_000_000;
    pub(super) const DEFAULT_SALT: &[u8; 4] = b"salt";
    pub(super) const EMPTY_PAYLOAD: &[u8; 0] = b"";
    pub(super) const OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS: u64 = 10000000;

    pub(super) type DispatchCustomResult<T> = Result<T, DispatchErrorWithPostInfo>;
    pub(super) type AccountId = <Test as frame_system::Config>::AccountId;
    pub(super) type GasPrice = <Test as pallet::Config>::GasPrice;

    type BlockNumber = <Test as frame_system::Config>::BlockNumber;

    pub(super) fn hash(data: impl AsRef<[u8]>) -> [u8; 32] {
        sp_core::blake2_256(data.as_ref())
    }

    pub fn init_logger() {
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    #[track_caller]
    pub(super) fn assert_balance(
        origin: impl common::Origin,
        free: impl Into<BalanceOf<Test>>,
        reserved: impl Into<BalanceOf<Test>>,
    ) {
        let account_id = AccountId::from_origin(origin.into_origin());
        assert_eq!(Balances::free_balance(account_id), free.into());
        assert_eq!(Balances::reserved_balance(account_id), reserved.into());
    }

    pub(super) fn calculate_handle_and_send_with_extra(
        origin: AccountId,
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: Option<u64>,
        value: BalanceOf<Test>,
    ) -> (MessageId, GasInfo) {
        let gas_info = Gear::calculate_gas_info(
            origin.into_origin(),
            HandleKind::Handle(destination),
            payload.clone(),
            value,
            true,
        )
        .expect("calculate_gas_info failed");

        let limit = gas_info.min_limit + gas_limit.unwrap_or_default();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(origin),
            destination,
            payload,
            limit,
            value
        ));

        let message_id = get_last_message_id();

        (message_id, gas_info)
    }

    pub(super) fn get_ed() -> u128 {
        <Test as pallet::Config>::Currency::minimum_balance().unique_saturated_into()
    }

    #[track_caller]
    pub(super) fn assert_init_success(expected: u32) {
        let mut actual_children_amount = 0;
        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::ProgramChanged {
                change: ProgramChangeKind::Active { .. },
                ..
            }) = e.event
            {
                actual_children_amount += 1
            }
        });

        assert_eq!(expected, actual_children_amount);
    }

    #[track_caller]
    pub(super) fn assert_last_dequeued(expected: u32) {
        let last_dequeued = System::events()
            .iter()
            .filter_map(|e| {
                if let MockRuntimeEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                    Some(total)
                } else {
                    None
                }
            })
            .last()
            .expect("Not found RuntimeEvent::MessagesDispatched");

        assert_eq!(expected, last_dequeued);
    }

    #[track_caller]
    pub(super) fn assert_total_dequeued(expected: u32) {
        let actual_dequeued: u32 = System::events()
            .iter()
            .filter_map(|e| {
                if let MockRuntimeEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
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
    #[track_caller]
    pub(super) fn setup_mailbox_test_state(user: AccountId) -> MessageId {
        let prog_id = {
            let res = upload_program_default(user, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        increase_prog_balance_for_mailbox_test(user, prog_id);
        populate_mailbox_from_program(prog_id, user, 2, 2_000_000_000, 0)
    }

    // Puts message from `prog_id` for the `user` in mailbox and returns its id
    #[track_caller]
    pub(super) fn populate_mailbox_from_program(
        prog_id: ProgramId,
        sender: AccountId,
        block_num: BlockNumber,
        gas_limit: u64,
        value: u128,
    ) -> MessageId {
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(sender),
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
                ProgramStorageOf::<Test>::get_program(prog_id)
                    .and_then(|(p, _bn)| common::ActiveProgram::try_from(p).ok())
                    .expect("program must exist")
                    .code_hash,
                generate_code_hash(&expected_code).into(),
                "can invoke send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
            );
        }

        MessageId::generate_outgoing(message_id, 0)
    }

    #[track_caller]
    pub(super) fn increase_prog_balance_for_mailbox_test(sender: AccountId, program_id: ProgramId) {
        let expected_code_hash: H256 = generate_code_hash(
            ProgramCodeKind::OutgoingWithValueInHandle
                .to_bytes()
                .as_slice(),
        )
        .into();
        let actual_code_hash = ProgramStorageOf::<Test>::get_program(program_id)
            .and_then(|(p, _bn)| common::ActiveProgram::try_from(p).ok())
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
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &sender,
            &AccountId::from_origin(program_id.into_origin()),
            locked_value,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));
    }

    // Submits program with default options (salt, gas limit, value, payload)
    #[track_caller]
    pub(super) fn upload_program_default(
        user: AccountId,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<ProgramId> {
        let code = code_kind.to_bytes();
        let salt = DEFAULT_SALT.to_vec();

        Gear::upload_program(
            RuntimeOrigin::signed(user),
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
        Gear::send_message(
            RuntimeOrigin::signed(from),
            to,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
        )
    }

    pub(super) fn call_default_message(to: ProgramId) -> crate::mock::RuntimeCall {
        crate::mock::RuntimeCall::Gear(crate::Call::<Test>::send_message {
            destination: to,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: DEFAULT_GAS_LIMIT,
            value: 0,
        })
    }

    pub(super) fn dispatch_status(message_id: MessageId) -> Option<DispatchStatus> {
        let mut found_status: Option<DispatchStatus> = None;
        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessagesDispatched { statuses, .. }) = &e.event {
                found_status = statuses.get(&message_id).map(Clone::clone);
            }
        });

        found_status
    }

    #[track_caller]
    pub(super) fn assert_dispatched(message_id: MessageId) {
        assert!(dispatch_status(message_id).is_some())
    }

    #[track_caller]
    pub(super) fn assert_succeed(message_id: MessageId) {
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::Success)
    }

    fn get_last_event_error_and_status_code(message_id: MessageId) -> (String, StatusCode) {
        let mut actual_error = None;

        System::events().into_iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                if let Some(details) = message.reply() {
                    if details.reply_to() == message_id && details.status_code() != 0 {
                        actual_error = Some((
                            String::from_utf8(message.payload().to_vec())
                                .expect("Unable to decode string from error reply"),
                            details.status_code(),
                        ));
                    }
                }
            }
        });

        let (mut actual_error, status_code) =
            actual_error.expect("Error message not found in any `RuntimeEvent::UserMessageSent`");

        log::debug!("Actual error: {:?}", actual_error);

        (actual_error, status_code)
    }

    #[track_caller]
    pub(super) fn get_last_event_error(message_id: MessageId) -> String {
        get_last_event_error_and_status_code(message_id).0
    }

    #[derive(derive_more::Display, derive_more::From)]
    pub(super) enum AssertFailedError {
        Execution(ActorExecutionErrorReason),
        SimpleReply(SimpleReplyError),
    }

    #[track_caller]
    pub(super) fn assert_failed(message_id: MessageId, error: impl Into<AssertFailedError>) {
        let error = error.into();
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::Failed, "Expected: {error}");

        let (mut actual_error, status_code) = get_last_event_error_and_status_code(message_id);
        let mut expectations = error.to_string();

        // In many cases fallible syscall returns ExtError, which program unwraps afterwards.
        // This check handles display of the error inside.
        if actual_error.starts_with('\'') {
            let j = actual_error.rfind('\'').expect("Checked above");
            actual_error = String::from(&actual_error[..(j + 1)]);
            expectations = format!("'{expectations}'");
        }

        assert_eq!(expectations, actual_error);

        if let AssertFailedError::SimpleReply(err) = error {
            assert_eq!(SimpleReplyError::from_status_code(status_code), Some(err));
        }
    }

    #[track_caller]
    pub(super) fn assert_not_executed(message_id: MessageId) {
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::NotExecuted)
    }

    #[track_caller]
    pub(super) fn get_last_event() -> MockRuntimeEvent {
        System::events()
            .into_iter()
            .last()
            .expect("failed to get last event")
            .event
    }

    #[track_caller]
    pub(super) fn get_last_program_id() -> ProgramId {
        let event = match System::events().last().map(|r| r.event.clone()) {
            Some(MockRuntimeEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        if let Event::MessageQueued {
            destination,
            entry: MessageEntry::Init,
            ..
        } = event
        {
            destination
        } else {
            unreachable!("expect RuntimeEvent::InitMessageEnqueued")
        }
    }

    #[track_caller]
    pub(super) fn get_last_code_id() -> CodeId {
        let event = match System::events().last().map(|r| r.event.clone()) {
            Some(MockRuntimeEvent::Gear(e)) => e,
            _ => unreachable!("Should be one Gear event"),
        };

        if let Event::CodeChanged {
            change: CodeChangeKind::Active { .. },
            id,
            ..
        } = event
        {
            id
        } else {
            unreachable!("expect Event::CodeChanged")
        }
    }

    #[track_caller]
    pub(super) fn filter_event_rev<F, R>(f: F) -> R
    where
        F: Fn(Event<Test>) -> Option<R>,
    {
        System::events()
            .iter()
            .rev()
            .filter_map(|r| {
                if let MockRuntimeEvent::Gear(e) = r.event.clone() {
                    Some(e)
                } else {
                    None
                }
            })
            .find_map(f)
            .expect("can't find message send event")
    }

    #[track_caller]
    pub(super) fn get_last_message_id() -> MessageId {
        System::events()
            .iter()
            .rev()
            .filter_map(|r| {
                if let MockRuntimeEvent::Gear(e) = r.event.clone() {
                    Some(e)
                } else {
                    None
                }
            })
            .find_map(|e| match e {
                Event::MessageQueued { id, .. } => Some(id),
                Event::UserMessageSent { message, .. } => Some(message.id()),
                _ => None,
            })
            .expect("can't find message send event")
    }

    #[track_caller]
    pub(super) fn get_waitlist_expiration(message_id: MessageId) -> BlockNumberFor<Test> {
        let mut exp = None;
        System::events()
            .into_iter()
            .rfind(|e| match e.event {
                MockRuntimeEvent::Gear(Event::MessageWaited {
                    id: message_id,
                    expiration,
                    ..
                }) => {
                    exp = Some(expiration);
                    true
                }
                _ => false,
            })
            .expect("Failed to find appropriate MessageWaited event");

        exp.unwrap()
    }

    #[track_caller]
    pub(super) fn get_last_message_waited() -> (MessageId, BlockNumberFor<Test>) {
        let mut message_id = None;
        let mut exp = None;
        System::events()
            .into_iter()
            .rfind(|e| {
                if let MockRuntimeEvent::Gear(Event::MessageWaited { id, expiration, .. }) = e.event
                {
                    message_id = Some(id);
                    exp = Some(expiration);
                    true
                } else {
                    false
                }
            })
            .expect("Failed to find appropriate MessageWaited event");

        (message_id.unwrap(), exp.unwrap())
    }

    #[track_caller]
    pub(super) fn maybe_last_message(account: AccountId) -> Option<StoredMessage> {
        System::events().into_iter().rev().find_map(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                if message.destination() == account.into() {
                    Some(message)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    #[track_caller]
    pub(super) fn maybe_any_last_message() -> Option<StoredMessage> {
        System::events().into_iter().rev().find_map(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                Some(message)
            } else {
                None
            }
        })
    }

    #[track_caller]
    pub(super) fn get_last_mail(account: AccountId) -> StoredMessage {
        MailboxOf::<Test>::iter_key(account)
            .last()
            .map(|(msg, _bn)| msg)
            .expect("Element should be")
    }

    #[track_caller]
    pub(super) fn get_reservation_map(pid: ProgramId) -> Option<GasReservationMap> {
        let (prog, _bn) = ProgramStorageOf::<Test>::get_program(pid).unwrap();
        if let common::Program::Active(common::ActiveProgram {
            gas_reservation_map,
            ..
        }) = prog
        {
            Some(gas_reservation_map)
        } else {
            None
        }
    }

    #[derive(Debug, Copy, Clone)]
    pub(super) enum ProgramCodeKind<'a> {
        Default,
        Custom(&'a str),
        CustomInvalid(&'a str),
        GreedyInit,
        OutgoingWithValueInHandle,
    }

    impl<'a> ProgramCodeKind<'a> {
        pub(super) fn to_bytes(self) -> Vec<u8> {
            let mut validate = true;
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
                            local.set $counter
                            loop $while
                                local.get $counter
                                i32.const 1
                                i32.add
                                local.set $counter
                                local.get $counter
                                local.get $size
                                i32.lt_s
                                if
                                    br $while
                                end
                            end $while
                        )
                        (func $init
                            i32.const 0x7fff_ffff
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
                        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
                        (import "env" "memory" (memory 1))
                        (export "handle" (func $handle))
                        (export "init" (func $init))
                        (export "handle_reply" (func $handle_reply))
                        (func $handle
                            i32.const 111 ;; addr
                            i32.const 1 ;; value
                            i32.store

                            i32.const 143 ;; addr + 32
                            i32.const 1000
                            i32.store

                            (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i64.const 10000000) (i32.const 0) (i32.const 333))

                            i32.const 333 ;; addr
                            i32.load
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
                ProgramCodeKind::CustomInvalid(code) => {
                    validate = false;
                    code
                }
            };

            wabt::Wat2Wasm::new()
                .validate(validate)
                .convert(source)
                .expect("failed to parse module")
                .as_ref()
                .to_vec()
        }
    }

    pub(super) fn print_gear_events() {
        let v = System::events()
            .into_iter()
            .map(|r| r.event)
            .collect::<Vec<_>>();

        println!("Gear events");
        for (pos, line) in v.iter().enumerate() {
            println!("{pos}). {line:?}");
        }
    }

    pub(super) fn waiting_init_messages(pid: ProgramId) -> Vec<MessageId> {
        ProgramStorageOf::<Test>::waiting_init_get_messages(pid)
    }
}

#[test]
fn check_gear_stack_end_fail() {
    // This test checks, that in case user makes WASM file with incorrect
    // gear stack end export, then execution will end with an error.
    let wat_template = |addr| {
        format!(
            r#"
            (module
                (import "env" "memory" (memory 4))
                (export "init" (func $init))
                (func $init)
                (global (;0;) (mut i32) (i32.const {addr}))
                (export "{STACK_END_EXPORT_NAME}" (global 0))
            )"#,
        )
    };

    init_logger();
    new_test_ext().execute_with(|| {
        // Check error when stack end bigger then static mem size
        let wat = wat_template(0x50000);
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat.as_str()).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_next_block(None);
        assert_last_dequeued(1);
        assert_failed(
            message_id,
            ActorExecutionErrorReason::PrepareMemory(
                ActorPrepareMemoryError::StackEndPageBiggerWasmMemSize(
                    WasmPage::new(5).unwrap(),
                    WasmPage::new(4).unwrap(),
                ),
            ),
        );

        // Check error when stack end is not aligned
        let wat = wat_template(0x10001);
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat.as_str()).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_next_block(None);
        assert_last_dequeued(1);
        assert_failed(
            message_id,
            ActorExecutionErrorReason::PrepareMemory(ActorPrepareMemoryError::StackIsNotAligned(
                65537,
            )),
        );

        // Check OK if stack end is suitable
        let wat = wat_template(0x10000);
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat.as_str()).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_next_block(None);
        assert_last_dequeued(1);
        assert_succeed(message_id);
    });
}

/// Test that error is generated in case `gr_read` requests out of bounds data from message.
#[test]
fn check_gr_read_error_works() {
    let wat = r#"
        (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_read" (func $gr_read (param i32 i32 i32 i32)))
            (export "init" (func $init))
            (func $init
                (call $gr_read (i32.const 0) (i32.const 10) (i32.const 0) (i32.const 111))

                i32.const 111
                i32.load
                (if ;; validating that error len is not zero
                    (then)
                    (else
                        unreachable
                    )
                )
            )
        )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_block(2, None);
        assert_succeed(message_id);
    });
}

/// Check that too large message, which is constructed by `gr_reply_push`,
/// leads to program execution error.
#[test]
fn check_reply_push_payload_exceed() {
    let wat = r#"
        (module
            (import "env" "memory" (memory 0x101))
            (import "env" "gr_reply_push" (func $gr (param i32 i32 i32)))
            (export "init" (func $init))
            (func $init
                ;; first reply push must be ok
                (block
                    (call $gr (i32.const 0) (i32.const 0x1000000) (i32.const 0x1000001))

                    (i32.load (i32.const 0x1000001))
                    i32.eqz
                    br_if 0
                    unreachable
                )
                ;; second must lead to overflow
                (block
                    (call $gr (i32.const 0) (i32.const 0x1000000) (i32.const 0x1000001))

                    (i32.load (i32.const 0x1000001))
                    i32.eqz
                    br_if 1
                    unreachable
                )
            )
        )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_block(2, None);
        assert_last_dequeued(1);

        assert_failed(
            message_id,
            ActorExecutionErrorReason::Trap(TrapExplanation::Unknown),
        );
    });
}

/// Check that random works and it's changing on next epoch.
#[test]
fn check_random_works() {
    use blake2_rfc::blake2b::blake2b;
    let wat = r#"
        (module
            (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
            (import "env" "gr_source" (func $gr_source (param i32)))
            (import "env" "gr_random" (func $gr_random (param i32 i32)))
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (func $handle
                (i32.store (i32.const 111) (i32.const 1))

                (call $gr_random (i32.const 0) (i32.const 64))

                (call $send (i32.const 111) (i32.const 68) (i32.const 32) (i64.const 10000000) (i32.const 0) (i32.const 333))

                (i32.load (i32.const 333))
                i32.eqz
                br_if 0
                unreachable
            )
        )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .expect("Failed to upload program");

        let sender = utils::get_last_program_id();

        let mut random_data = Vec::new();

        (1..10).for_each(|_| {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                sender,
                EMPTY_PAYLOAD.to_vec(),
                50_000_000_000,
                0,
            ));

            let output: ([u8; 32], u64) =
                <Test as Config>::Randomness::random(get_last_message_id().as_ref());

            random_data.push([[0; 32], output.0].concat());
            run_to_block(System::block_number() + 1, None);
        });

        assert_eq!(random_data.len(), MailboxOf::<Test>::len(&USER_1));

        let mut sorted_mailbox: Vec<(gear_core::message::StoredMessage, Interval<u64>)> =
            MailboxOf::<Test>::iter_key(USER_1).collect();
        sorted_mailbox.sort_by(|a, b| a.1.finish.cmp(&b.1.finish));

        sorted_mailbox
            .iter()
            .zip(random_data.iter())
            .for_each(|((msg, _bn), random_data)| {
                assert_eq!(blake2b(32, &[], random_data).as_bytes(), msg.payload());
            });

        // // assert_last_dequeued(1);
        // println!("{:?}", res);
        // assert_eq!(blake2b(32, &[], &output.0.encode()).as_bytes(), res.payload());
    });
}

#[test]
fn relay_messages() {
    use demo_proxy_relay::{RelayCall, ResendPushData, WASM_BINARY};

    struct Expected {
        user: AccountId,
        payload: Vec<u8>,
    }

    let source = USER_1;

    init_logger();
    let test = |relay_call: RelayCall, payload: &[u8], expected: Vec<Expected>| {
        let execute = || {
            System::reset_events();

            let label = format!("{relay_call:?}");
            assert!(
                Gear::upload_program(
                    RuntimeOrigin::signed(source),
                    WASM_BINARY.to_vec(),
                    vec![],
                    relay_call.encode(),
                    50_000_000_000u64,
                    0u128
                )
                .is_ok(),
                "{}",
                label
            );

            let proxy = utils::get_last_program_id();

            run_to_next_block(None);

            assert!(Gear::is_active(proxy), "{}", label);

            assert!(
                Gear::send_message(
                    RuntimeOrigin::signed(source),
                    proxy,
                    payload.to_vec(),
                    DEFAULT_GAS_LIMIT * 10,
                    0,
                )
                .is_ok(),
                "{}",
                label
            );

            run_to_next_block(None);

            for Expected { user, payload } in expected {
                assert_eq!(
                    MailboxOf::<Test>::drain_key(user)
                        .next()
                        .map(|(msg, _bn)| msg.payload().to_vec()),
                    Some(payload),
                    "{label}",
                );
            }
        };

        new_test_ext().execute_with(execute);
    };

    let payload = b"Hi, USER_2! Ping USER_3.";
    let relay_call = RelayCall::ResendPush(vec![
        // "Hi, USER_2!"
        ResendPushData {
            destination: USER_2.into(),
            start: None,
            end: Some((10, true)),
        },
        // the same but end index specified in another way
        ResendPushData {
            destination: USER_2.into(),
            start: None,
            end: Some((11, false)),
        },
        // "Ping USER_3."
        ResendPushData {
            destination: USER_3.into(),
            start: Some(12),
            end: None,
        },
        // invalid range
        ResendPushData {
            destination: USER_3.into(),
            start: Some(2),
            end: Some((0, true)),
        },
        // invalid range
        ResendPushData {
            destination: USER_3.into(),
            start: Some(payload.len() as u32),
            end: Some((0, false)),
        },
    ]);

    let expected = vec![
        Expected {
            user: USER_2,
            payload: payload[..11].to_vec(),
        },
        Expected {
            user: USER_2,
            payload: payload[..11].to_vec(),
        },
        Expected {
            user: USER_3,
            payload: payload[12..].to_vec(),
        },
        Expected {
            user: USER_3,
            payload: vec![],
        },
        Expected {
            user: USER_3,
            payload: vec![],
        },
    ];

    test(relay_call, payload, expected);

    test(
        RelayCall::Resend(USER_3.into()),
        payload,
        vec![Expected {
            user: USER_3,
            payload: payload.to_vec(),
        }],
    );
    test(
        RelayCall::ResendWithGas(USER_3.into(), 50_000),
        payload,
        vec![Expected {
            user: USER_3,
            payload: payload.to_vec(),
        }],
    );

    test(
        RelayCall::Rereply,
        payload,
        vec![Expected {
            user: source,
            payload: payload.to_vec(),
        }],
    );
    test(
        RelayCall::RereplyPush,
        payload,
        vec![Expected {
            user: source,
            payload: payload.to_vec(),
        }],
    );
    test(
        RelayCall::RereplyWithGas(60_000),
        payload,
        vec![Expected {
            user: source,
            payload: payload.to_vec(),
        }],
    );
}

#[test]
fn module_instantiation_error() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init)
        (data (;0;) (i32.const -15186172) "\b9w\92")
    )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat).to_bytes();
        let salt = DEFAULT_SALT.to_vec();
        let prog_id = generate_program_id(&code, &salt);
        let res = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .map(|_| prog_id);
        let mid = get_last_message_id();

        assert_ok!(res);

        run_to_next_block(None);

        assert!(Gear::is_terminated(prog_id));
        let err = get_last_event_error(mid);
        assert!(err.starts_with(&ActorExecutionErrorReason::Environment("".into()).to_string()));
    });
}

#[test]
fn wrong_entry_type() {
    let wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (func $init (param i32))
    )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            50_000_000_000,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        let err = get_last_event_error(mid);
        assert!(err.starts_with(&ActorExecutionErrorReason::Environment("".into()).to_string()));
    });
}

#[test]
fn oom_handler_works() {
    use demo_out_of_memory::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000_000_u64,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ActorExecutionErrorReason::Trap(TrapExplanation::ProgramAllocOutOfBounds),
        );
    });
}

#[test]
#[ignore = "TODO: return this test if it's possible after #2226, or remove it."]
fn alloc_charge_error() {
    const WAT: &str = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "alloc" (func $alloc (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        ;; we are trying to allocate so many pages with such small gas limit
        ;; that we will get `GasLimitExceeded` error
        i32.const 0xff
        call $alloc
        drop
    )
)
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(WAT).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            500_000_000_u64,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ActorExecutionErrorReason::Trap(TrapExplanation::GasLimitExceeded),
        );
    });
}

#[test]
fn free_usage_error() {
    const WAT: &str = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "free" (func $free (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        ;; free impossible and non-existing page
        i32.const 0xffffffff
        call $free
        ;; free must return 1 so we will get `unreachable` instruction
        i32.const 0
        i32.eq
        br_if 0
        unreachable
    )
)
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(WAT).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            500_000_000_u64,
            0,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ActorExecutionErrorReason::Trap(TrapExplanation::Unknown),
        );
    });
}

#[test]
fn reject_incorrect_stack_pointer() {
    let wat = format!(
        r#"
(module
    (import "env" "memory" (memory 1))
    (func $init)
    (global (;0;) i32 (i32.const 65536))
    (export "init" (func $init))
    (export "{STACK_END_EXPORT_NAME}" (global 0))
    (data $.rodata (i32.const 60000) "GEAR")
)
    "#
    );

    init_logger();
    new_test_ext().execute_with(|| {
        assert_noop!(
            Gear::upload_code(
                RuntimeOrigin::signed(USER_1),
                ProgramCodeKind::CustomInvalid(&wat).to_bytes()
            ),
            Error::<Test>::ProgramConstructionFailed
        );

        assert_noop!(
            upload_program_default(USER_1, ProgramCodeKind::CustomInvalid(&wat)),
            Error::<Test>::ProgramConstructionFailed
        );
    });
}
