// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    AccountIdOf, BlockGasLimitOf, Config, CostsPerBlockOf, CurrencyOf, DbWeightOf, DispatchStashOf,
    Error, Event, ExtManager, GasAllowanceOf, GasBalanceOf, GasHandlerOf, GasInfo, GearBank,
    Limits, MailboxOf, ProgramStorageOf, QueueOf, Schedule, TaskPoolOf, WaitlistOf,
    builtin::BuiltinDispatcherFactory,
    internal::{HoldBound, HoldBoundBuilder, InheritorForError},
    manager::HandleKind,
    mock::{
        self, BLOCK_AUTHOR, Balances, BlockNumber, DynamicSchedule, Gear, GearVoucher,
        LOW_BALANCE_USER, RENT_POOL, RuntimeEvent as MockRuntimeEvent, RuntimeOrigin, System, Test,
        USER_1, USER_2, USER_3, new_test_ext, run_for_blocks, run_to_block,
        run_to_block_maybe_with_queue, run_to_next_block,
    },
    pallet,
    runtime_api::{ALLOWANCE_LIMIT_ERR, RUNTIME_API_BLOCK_LIMITS_COUNT},
};
use common::{
    CodeStorage, GasTree, GearPage, LockId, LockableTree, Origin as _, Program, ProgramStorage,
    ReservableTree, event::*, scheduler::*, storage::*,
};
use demo_constructor::{Calls, Scheme};
use frame_support::{
    assert_noop, assert_ok,
    sp_runtime::traits::{TypedGet, Zero},
    traits::{Currency, Randomness},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    buffer::Payload,
    code::{
        self, Code, CodeError, ExportError, InstrumentedCodeAndMetadata, MAX_WASM_PAGES_AMOUNT,
    },
    gas_metering::CustomConstantCostRules,
    ids::{ActorId, CodeId, MessageId, prelude::*},
    memory::PageBuf,
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext,
        StoredDispatch, UserStoredMessage,
    },
    pages::{
        WasmPage, WasmPagesAmount,
        numerated::{self, tree::IntervalsTree},
    },
    program::ActiveProgram,
    rpc::ReplyInfo,
    tasks::ScheduledTask,
};
use gear_core_backend::error::TrapExplanation;
use gear_core_errors::*;
use gear_wasm_instrument::{Instruction, Module, STACK_END_EXPORT_NAME};
use gstd::{
    collections::BTreeMap,
    errors::{CoreError, Error as GstdError},
};
use pallet_gear_voucher::PrepaidCall;
use sp_core::H256;
use sp_runtime::{
    SaturatedConversion,
    codec::{Decode, Encode},
    traits::{Dispatchable, One, UniqueSaturatedInto},
};
use sp_std::convert::TryFrom;
use std::{collections::BTreeSet, num::NonZero};
pub use utils::init_logger;
use utils::*;

type Gas = <<Test as Config>::GasProvider as common::GasProvider>::GasTree;

#[test]
fn err_reply_comes_with_value() {
    init_logger();
    new_test_ext().execute_with(|| {
        const VALUE: u128 = 10_000_000_000_000;

        let (_init_mid, pid) = init_constructor(Scheme::empty());

        // Case #1.
        // If reply-able message quits with error, value is attached to the reply.
        let user_balance = Balances::free_balance(USER_1);
        assert_eq!(Balances::free_balance(pid.cast::<AccountId>()), get_ed());

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            0,
            VALUE,
            false,
        ));
        assert_balance(USER_1, user_balance - VALUE, VALUE);
        assert_eq!(Balances::free_balance(pid.cast::<AccountId>()), get_ed());

        run_to_next_block(None);

        assert_balance(USER_1, user_balance, 0u8);
        assert_eq!(Balances::free_balance(pid.cast::<AccountId>()), get_ed());

        let err_reply = maybe_last_message(USER_1).expect("Message should be");

        assert_eq!(
            err_reply.reply_code().expect("must be"),
            ReplyCode::Error(ErrorReplyReason::Execution(
                SimpleExecutionError::RanOutOfGas
            ))
        );

        assert_eq!(err_reply.value(), VALUE);

        // Cases #2-3.
        // If non-reply-able message quits with error, value is kept by program.
        //
        // Case #2: success reply quits with error.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            pid,
            Calls::builder()
                .send(USER_1.into_origin().0, b"Hello, world!".to_vec())
                .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        let mail = get_last_mail(USER_1);

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            mail.id(),
            vec![],
            0,
            VALUE,
            false,
        ));

        assert_balance(USER_1, user_balance - VALUE, VALUE);

        run_to_next_block(None);

        let pid_balance = Balances::free_balance(pid.cast::<AccountId>());
        assert_eq!(pid_balance, get_ed() + VALUE);

        assert_balance(USER_1, user_balance - VALUE, 0u8);
        assert_balance(pid, pid_balance, 0u8);

        // Case #3: error reply quits with error.
        const VALUE_2: u128 = 15_000_000_000_000;

        let scheme = Scheme::predefined(
            Calls::builder().noop(),
            Calls::builder().send_value(
                pid.into_bytes(),
                Calls::builder().panic(None).encode(),
                VALUE_2,
            ),
            Calls::builder().panic(None),
            Calls::builder().noop(),
        );

        let (_, pid2) =
            submit_constructor_with_args(USER_1, H256::random().as_bytes(), scheme, VALUE_2);

        run_to_next_block(None);
        assert!(is_active(pid2));

        let pid2_balance = Balances::free_balance(pid2.cast::<AccountId>());
        assert_eq!(pid2_balance, get_ed() + VALUE_2);

        assert_balance(pid, pid_balance, 0u8);
        assert_balance(pid2, pid2_balance, 0u8);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            vec![],
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        assert_last_dequeued(3);

        assert_balance(pid, pid_balance, 0u8);
        assert_balance(pid2, pid2_balance, 0u8);

        // Case #4.
        // Exited program returns value as well.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            pid,
            Calls::builder().exit(USER_2.into_origin().0).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let user_balance = Balances::free_balance(USER_1);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            0,
            1,
            false,
        ));

        run_to_next_block(None);

        let mail = maybe_last_message(USER_1).expect("Message should be");
        assert_eq!(mail.value(), 1);

        assert_balance(USER_1, user_balance, 0u8);
        assert_balance(pid, 0u8, 0u8);
    })
}

#[test]
fn auto_reply_on_exit_exists() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, pid) =
            init_constructor(Scheme::with_handle(Calls::builder().exit([1; 32])));

        run_to_next_block(None);

        let res = Gear::calculate_reply_for_handle(USER_1, pid, vec![], 100_000_000_000, 0)
            .expect("Failed to query reply");

        assert_eq!(
            res,
            ReplyInfo {
                payload: vec![],
                value: 0,
                code: ReplyCode::Success(SuccessReplyReason::Auto)
            }
        );
    });
}

#[test]
fn calculate_reply_for_handle_works() {
    use demo_constructor::demo_ping;

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, ping_pong) = init_constructor(demo_ping::scheme());

        run_to_next_block(None);

        // Happy case.
        let res = Gear::calculate_reply_for_handle(
            USER_1,
            ping_pong,
            b"PING".to_vec(),
            100_000_000_000,
            0,
        )
        .expect("Failed to query reply");

        assert_eq!(
            res,
            ReplyInfo {
                payload: b"PONG".to_vec(),
                value: 0,
                code: ReplyCode::Success(SuccessReplyReason::Manual)
            }
        );

        // Out of gas panic case.
        let res =
            Gear::calculate_reply_for_handle(USER_1, ping_pong, b"PING".to_vec(), 700_000_000, 0)
                .expect("Failed to query reply");

        assert_eq!(
            res,
            ReplyInfo {
                payload: vec![],
                value: 0,
                code: ReplyCode::Error(ErrorReplyReason::Execution(
                    SimpleExecutionError::RanOutOfGas
                )),
            }
        );

        // TODO: uncomment code below (issue #3804).
        // // Value returned in case of error.
        // let value = get_ed() * 2;
        // let res = Gear::calculate_reply_for_handle(
        //     USER_1,
        //     ping_pong,
        //     vec![],
        //     0,
        //     value,
        // ).expect("Failed to query reply");
        // assert_eq!(res.value, value);
    })
}

#[test]
fn calculate_gas_results_in_finite_wait() {
    use demo_constructor::{Calls, Scheme};

    // Imagine that this is async `send_for_reply` to some user
    // with wait up to 20 that is not rare case.
    let receiver_scheme = Scheme::with_handle(Calls::builder().wait_for(20));

    let sender_scheme = |receiver_id: ActorId| {
        Scheme::with_handle(Calls::builder().send_wgas(
            <[u8; 32]>::from(receiver_id),
            [],
            40_000_000_000u64,
        ))
    };

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, receiver) = init_constructor(receiver_scheme);
        let (_init_mid, sender) =
            submit_constructor_with_args(USER_1, "salty salt", sender_scheme(receiver), 0);

        run_to_next_block(None);

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(sender),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        // Original issue: this case used to return block gas limit as minimal.
        assert!(BlockGasLimitOf::<Test>::get() / 2 > min_limit);
    });
}

#[test]
fn state_rpc_calls_trigger_reinstrumentation() {
    use demo_fungible_token::{InitConfig, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Program uploading.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitConfig::test_sequence().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
            false,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        // Below goes invalidation of instrumented code for uploaded program
        // with following instrumentation version dump to check that
        // re-instrumentation takes place.

        /* starts here */
        let empty_wat = r#"
        (module
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $init)
            (func $handle)
        )
        "#;

        let schedule = <Test as Config>::Schedule::get();

        let code = Code::try_new(
            ProgramCodeKind::Custom(empty_wat).to_bytes(),
            0, // invalid version
            |module| schedule.rules(module),
            schedule.limits.stack_height,
            schedule.limits.data_segments_amount.into(),
        )
        .expect("Failed to create dummy code");

        let (_, instrumented_code, invalid_metadata) = code.into_parts();

        // Code metadata doesn't have to be completely wrong, just a version of instrumentation
        let old_code_metadata =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id).unwrap();
        let code_metadata = old_code_metadata.into_failed_instrumentation(
            invalid_metadata
                .instruction_weights_version()
                .expect("Failed to get instructions weight version"),
        );

        <Test as Config>::CodeStorage::update_instrumented_code_and_metadata(
            program.code_id,
            InstrumentedCodeAndMetadata {
                instrumented_code,
                metadata: code_metadata,
            },
        );
        /* ends here */

        assert_ok!(Gear::read_state_impl(program_id, Default::default(), None));
    });
}

#[test]
fn calculate_gas_init_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let err = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Init(ProgramCodeKind::GreedyInit.to_bytes()),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect_err("Expected program to fail due to lack of gas");

        assert!(err.starts_with("Program terminated with a trap"));
    });
}

#[test]
#[ignore = "TODO: enable me once #3665 solved: \
`gas_below_ed` atm is less than needed to initialize program"]
fn calculate_gas_zero_balance() {
    init_logger();
    new_test_ext().execute_with(|| {
        const ZERO_BALANCE_USER: AccountId = 12122023;
        assert!(Balances::free_balance(ZERO_BALANCE_USER).is_zero());

        let gas_below_ed = get_ed()
            .saturating_div(gas_price(1))
            .saturating_sub(One::one());

        assert_ok!(Gear::calculate_gas_info_impl(
            ZERO_BALANCE_USER.into_origin(),
            HandleKind::Init(ProgramCodeKind::Default.to_bytes()),
            gas_below_ed as u64,
            vec![],
            0,
            false,
            false,
            None,
        ));
    });
}

#[test]
fn test_failing_delayed_reservation_send() {
    use demo_delayed_reservation_sender::{
        ReservationSendingShowcase, SENDING_EXPECT, WASM_BINARY,
    };

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(pid));

        let reservation_amount = <Test as Config>::MailboxThreshold::get();
        let sending_delay = 1u32;
        let sending_delay_hold_bound: HoldBound<Test> =
            HoldBoundBuilder::new(StorageType::DispatchStash).duration(sending_delay as u64);
        let sending_delay_gas = sending_delay_hold_bound.lock_amount();
        // Sending delayed msg from a reservation checks: reservation_amount - delay_hold - mailbox_therhsold.
        // Current example sets reseravtion_amount to mailbox_threshold. So, obviously, sending message
        // from a reservation is impossible - reservation has insufficient gas reserved.
        assert_ne!(sending_delay_gas, 0);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            ReservationSendingShowcase::ToSourceInPlace {
                reservation_amount,
                reservation_delay: 1_000,
                sending_delay,
            }
            .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let mid = utils::get_last_message_id();

        run_to_next_block(None);
        let error_text = format!(
            "panicked with '{SENDING_EXPECT}: {:?}'",
            CoreError::Ext(ExtError::Message(
                MessageError::InsufficientGasForDelayedSending
            ))
        );
        assert_failed(mid, AssertFailedError::Panic(error_text));

        // Possibly sent message from reservation with a 1 block delay duration.
        let outgoing = MessageId::generate_outgoing(mid, 0);
        let err_reply = MessageId::generate_reply(mid);

        assert!(!DispatchStashOf::<Test>::contains_key(&outgoing));

        run_to_next_block(None);

        assert!(!DispatchStashOf::<Test>::contains_key(&outgoing));
        assert!(!MailboxOf::<Test>::contains(&USER_1, &outgoing));

        let message = maybe_any_last_message().expect("Should be");
        assert_eq!(message.id(), err_reply);
        assert_eq!(message.destination(), USER_1.cast());
    });
}

#[test]
fn cascading_delayed_gasless_send_work() {
    use demo_delayed_sender::{DELAY, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            0u32.to_le_bytes().to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(pid));

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        // Case when trying to send one of them to mailbox.
        // Fails as any gasless message sending costs reducing
        // mailbox threshold from the gas counter
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            min_limit - <Test as Config>::MailboxThreshold::get(),
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );

        // Similar case when two of two goes into mailbox.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            min_limit,
            0,
            false,
        ));

        let mid = get_last_message_id();

        let first_outgoing = MessageId::generate_outgoing(mid, 0);
        let second_outgoing = MessageId::generate_outgoing(mid, 1);

        run_to_next_block(None);

        assert_succeed(mid);

        run_for_blocks(DELAY as u64, None);
        assert!(MailboxOf::<Test>::contains(&USER_1, &first_outgoing));
        assert!(MailboxOf::<Test>::contains(&USER_1, &second_outgoing));

        // Similar case when none of them goes into mailbox
        // (impossible because delayed sent after gasless).
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            min_limit - 2 * <Test as Config>::MailboxThreshold::get(),
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );
    });
}

#[test]
fn calculate_gas_delayed_reservations_sending() {
    use demo_delayed_reservation_sender::{ReservationSendingShowcase, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(pid));

        // I. In-place case
        assert!(
            Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(pid),
                ReservationSendingShowcase::ToSourceInPlace {
                    reservation_amount: 10 * <Test as Config>::MailboxThreshold::get(),
                    reservation_delay: 1_000,
                    sending_delay: 10,
                }
                .encode(),
                0,
                true,
                true,
            )
            .is_ok()
        );

        // II. After-wait case (never failed before, added for test coverage).
        assert!(
            Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(pid),
                ReservationSendingShowcase::ToSourceAfterWait {
                    reservation_amount: 10 * <Test as Config>::MailboxThreshold::get(),
                    reservation_delay: 1_000,
                    wait_for: 3,
                    sending_delay: 10,
                }
                .encode(),
                0,
                true,
                true,
            )
            .is_ok()
        );
    });
}

#[test]
fn delayed_reservations_sending_validation() {
    use demo_delayed_reservation_sender::{
        ReservationSendingShowcase, SENDING_EXPECT, WASM_BINARY,
    };

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(pid));

        // I. In place sending can't appear if not enough gas limit in gas reservation.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            ReservationSendingShowcase::ToSourceInPlace {
                reservation_amount: 10 * <Test as Config>::MailboxThreshold::get(),
                reservation_delay: 1_000,
                sending_delay: (1_000 * <Test as Config>::MailboxThreshold::get()
                    + CostsPerBlockOf::<Test>::reserve_for()
                        / CostsPerBlockOf::<Test>::dispatch_stash())
                    as u32,
            }
            .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let mid = utils::get_last_message_id();

        run_to_next_block(None);

        let error_text = format!(
            "panicked with '{SENDING_EXPECT}: {:?}'",
            CoreError::Ext(ExtError::Message(
                MessageError::InsufficientGasForDelayedSending
            ))
        );

        assert_failed(mid, AssertFailedError::Panic(error_text));

        // II. After-wait sending can't appear if not enough gas limit in gas reservation.
        let wait_for = 5;

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            ReservationSendingShowcase::ToSourceAfterWait {
                reservation_amount: 10 * <Test as Config>::MailboxThreshold::get(),
                reservation_delay: 1_000,
                wait_for,
                sending_delay: (1_000 * <Test as Config>::MailboxThreshold::get()
                    + CostsPerBlockOf::<Test>::reserve_for()
                        / CostsPerBlockOf::<Test>::dispatch_stash())
                    as u32,
            }
            .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let mid = utils::get_last_message_id();

        run_for_blocks(wait_for as u64 + 1, None);

        let error_text = format!(
            "panicked with '{SENDING_EXPECT}: {:?}'",
            CoreError::Ext(ExtError::Message(
                MessageError::InsufficientGasForDelayedSending
            ))
        );

        assert_failed(mid, AssertFailedError::Panic(error_text));
    });
}

#[test]
fn delayed_reservations_to_mailbox() {
    use demo_delayed_reservation_sender::{ReservationSendingShowcase, WASM_BINARY};

    struct LockOrExpiration;

    impl LockOrExpiration {
        fn lock_for_stash(delay: u32) -> u64 {
            let stash_hold =
                Self::hold_bound_builder(StorageType::DispatchStash).duration(delay.into());

            stash_hold.lock_amount()
        }

        fn expiration_for_mailbox(gas: u64) -> u64 {
            let mailbox_hold = Self::hold_bound_builder(StorageType::Mailbox).maximum_for(gas);

            mailbox_hold.expected()
        }

        fn hold_bound_builder(storage_type: StorageType) -> HoldBoundBuilder<Test> {
            HoldBoundBuilder::<Test>::new(storage_type)
        }
    }

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(Gear::is_initialized(pid));

        let sending_delay = 10;
        let delay_lock_amount = LockOrExpiration::lock_for_stash(sending_delay);

        let reservation_amount = delay_lock_amount + 10 * <Test as Config>::MailboxThreshold::get();
        let reservation_expiration = LockOrExpiration::expiration_for_mailbox(reservation_amount);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            ReservationSendingShowcase::ToSourceInPlace {
                reservation_amount,
                reservation_delay: 1,
                sending_delay,
            }
            .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let mid = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(mid);

        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        run_for_blocks(sending_delay as u64, None);

        assert!(!MailboxOf::<Test>::is_empty(&USER_1));

        run_to_block(reservation_expiration, None);

        assert!(MailboxOf::<Test>::is_empty(&USER_1));
    });
}

#[test]
fn default_wait_lock_timeout() {
    use demo_async_tester::{Kind, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000u64,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            Kind::Send.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let mid = utils::get_last_message_id();

        run_to_next_block(None);

        let expiration_block = get_waitlist_expiration(mid);

        run_to_block(expiration_block, None);

        let error_text = format!(
            "panicked with 'ran into error-reply: {:?}'",
            GstdError::Timeout(
                expiration_block.unique_saturated_into(),
                expiration_block.unique_saturated_into()
            )
        );

        assert_failed(mid, AssertFailedError::Panic(error_text));
    })
}

#[test]
fn value_counter_set_correctly_for_interruptions() {
    use demo_constructor::{Arg, Calls, Scheme};

    // Equivalent of:
    //
    // use gstd::{msg, exec};
    //
    // #[unsafe(no_mangle)]
    // extern "C" fn handle() {
    //     msg::send(msg::source(), exec::value_available(), 0).unwrap();
    //     msg::send_bytes(Default::default(), [], msg::value()).unwrap();
    //     exec::wait_for(1);
    // }
    //
    // Message auto wakes on the next block after execution and
    // does everything again from the beginning.
    //
    // So for the first run we expect source to receive
    // `value_available` == `init_value` + msg value.
    // For second run we expect just `init_value`.
    let handle = Calls::builder()
        .source("source_store")
        .value("value_store")
        .value_available_as_vec("value_available_store")
        .send_wgas("source_store", "value_available_store", 0)
        .send_value(Arg::new([0u8; 32]), Arg::new(vec![]), "value_store")
        .wait_for(1);

    let scheme = Scheme::predefined(
        Calls::builder().noop(),
        handle,
        Calls::builder().noop(),
        Calls::builder().noop(),
    );

    init_logger();
    new_test_ext().execute_with(|| {
        const INIT_VALUE: u128 = 123_123_123;
        const VALUE: u128 = 10_000;

        let (_mid, pid) = init_constructor_with_value(scheme, INIT_VALUE);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            VALUE,
            false,
        ));

        run_to_next_block(None);
        let msg = maybe_last_message(USER_1).expect("Message should be");
        let value_available =
            u128::decode(&mut msg.payload_bytes()).expect("Failed to decode value available");
        assert_eq!(value_available, INIT_VALUE + VALUE);

        run_to_next_block(None);
        let msg = maybe_last_message(USER_1).expect("Message should be");
        let value_available =
            u128::decode(&mut msg.payload_bytes()).expect("Failed to decode value available");
        assert_eq!(value_available, INIT_VALUE);
    });
}

#[test]
fn calculate_gas_returns_not_block_limit() {
    use demo_program_generator::{CHILD_WAT, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let ed = get_ed();

        let code = ProgramCodeKind::Custom(CHILD_WAT).to_bytes();
        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), code));
        // Generator needs some balance to be able to pay ED for created programs
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            2 * ed,
            false,
        ));

        let generator_id = get_last_program_id();

        run_to_next_block(None);
        assert!(utils::is_active(generator_id));

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(generator_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ne!(min_limit, BlockGasLimitOf::<Test>::get());
    });
}

#[test]
fn read_big_state() {
    use demo_read_big_state::{State, Strings, WASM_BINARY};

    init_logger();

    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);
        assert!(utils::is_active(pid));

        let string = String::from("hi").repeat(4095);
        let string_size = 8 * 1024;
        assert_eq!(string.encoded_size(), string_size);

        let strings = Strings::new(string);
        let strings_size = (string_size * Strings::LEN) + 1;
        assert_eq!(strings.encoded_size(), strings_size);

        let approx_size =
            |size: usize, iteration: usize| -> usize { size - 17 - 144 * (iteration + 1) };

        // with initial data step is ~2 MiB
        let expected_size = |iteration: usize| -> usize {
            Strings::LEN * State::LEN * string_size * (iteration + 1)
        };

        // go to 6 MiB due to approximate calculations and 8MiB reply restrictions
        for i in 0..3 {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                strings.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));
            let mid = get_last_message_id();

            run_to_next_block(None);

            assert_succeed(mid);
            let state =
                Gear::read_state_impl(pid, Default::default(), None).expect("Failed to read state");
            assert_eq!(approx_size(state.len(), i), expected_size(i));
        }
    });
}

#[test]
fn auto_reply_sent() {
    init_logger();

    new_test_ext().execute_with(|| {
        // Init fn doesn't exist.
        // Handle function exists.
        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        run_to_next_block(None);

        assert!(utils::is_active(program_id));
        assert!(maybe_last_message(USER_1).is_some());
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            10_000,
            false,
        ));

        run_to_next_block(None);

        // asserting auto_reply
        assert!(System::events().into_iter().any(|e| {
            match e.event {
                MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. })
                    if message.destination().into_origin() == USER_1.into_origin() =>
                {
                    message
                        .reply_code()
                        .map(|code| code == ReplyCode::Success(SuccessReplyReason::Auto))
                        .unwrap_or(false)
                }
                _ => false,
            }
        }));

        // auto reply goes first (may be changed in future),
        // so atm we're allowed to get other message that way
        let id_to_reply = maybe_last_message(USER_1).unwrap().id();

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            id_to_reply,
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        System::reset_events();
        run_to_next_block(None);

        // reply dequeued
        assert_last_dequeued(1);
        // no auto reply sent
        assert!(maybe_any_last_message().is_none());
    })
}

#[test]
fn auto_reply_from_user_no_mailbox() {
    use demo_constructor::{Call, Calls, Scheme};

    init_logger();
    // no delay case
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor_id) = init_constructor(Scheme::empty());

        let calls = Calls::builder().send_wgas(<[u8; 32]>::from(USER_1.into_origin()), [], 0);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 1 init message + 1 handle message + 1 auto_reply to program on message sent to user
        assert_total_dequeued(3);
    });

    // delay case
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor_id) = init_constructor(Scheme::empty());

        let calls = Calls::builder().add_call(Call::Send(
            <[u8; 32]>::from(USER_1.into_origin()).into(),
            [].into(),
            Some(0u64.into()),
            0u128.into(),
            1u32.into(),
        ));
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 1 init message + 1 handle message
        assert_total_dequeued(2);

        run_to_next_block(None);
        // 1 init message + 1 handle message + 1 auto_reply to program on message sent to user with delay
        assert_total_dequeued(3);
    })
}

#[test]
fn auto_reply_out_of_rent_waitlist() {
    use demo_proxy::{InputArgs as ProxyInputArgs, WASM_BINARY as PROXY_WASM_BINARY};
    use demo_waiter::{Command, WASM_BINARY as WAITER_WASM_BINARY, WaitSubcommand};

    init_logger();

    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITER_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        let waiter_id = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ProxyInputArgs {
                destination: waiter_id.into(),
            }
            .encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        let proxy_id = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(waiter_id));
        assert!(utils::is_active(proxy_id));
        assert_total_dequeued(2); // 2 init messages
        assert_eq!(
            // 2 auto replies into USER_1 events
            System::events()
                .iter()
                .filter_map(|r| {
                    if let MockRuntimeEvent::Gear(Event::UserMessageSent {
                        message,
                        expiration: None,
                    }) = &r.event
                    {
                        (message.destination().into_origin() == USER_1.into_origin()
                            && message.reply_code() == Some(SuccessReplyReason::Auto.into()))
                        .then_some(())
                    } else {
                        None
                    }
                })
                .count(),
            2
        );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy_id,
            Command::Wait(WaitSubcommand::Wait).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        // Message to proxy program and its message to waiter.
        assert_last_dequeued(2);
        let (_msg_waited, expiration) = get_last_message_waited();

        // Hack to fast spend blocks till expiration.
        System::set_block_number(expiration - 1);
        Gear::set_block_number(expiration - 1);

        run_to_next_block(None);
        // Signal for waiter program since it has system reservation
        // + auto error reply to proxy program.
        assert_last_dequeued(2);
    });
}

#[test]
fn auto_reply_out_of_rent_mailbox() {
    init_logger();

    new_test_ext().execute_with(|| {
        let value = 1_000_u128;
        let ed = get_ed();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_3),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            value,
            false,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(utils::is_active(program_id));

        let user1_balance = Balances::free_balance(USER_1);
        assert_program_balance(program_id, value, ed, 0u128);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let message_id = utils::get_last_message_id();

        run_to_next_block(None);
        assert_succeed(message_id);

        assert_program_balance(program_id, 0_u128, ed, value);

        let mailed_msg = utils::get_last_mail(USER_1);
        let expiration = utils::get_mailbox_expiration(mailed_msg.id());

        // Hack to fast spend blocks till expiration.
        System::set_block_number(expiration - 1);
        Gear::set_block_number(expiration - 1);

        assert_eq!(user1_balance, Balances::free_balance(USER_1));

        run_to_block_maybe_with_queue(expiration, None, Some(false));
        assert_program_balance(program_id, 0u128, ed, 0u128);
        assert_eq!(user1_balance + value, Balances::free_balance(USER_1));

        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        // auto reply sent.
        let dispatch = QueueOf::<Test>::dequeue()
            .expect("Infallible")
            .expect("Should be");
        assert!(dispatch.payload_bytes().is_empty());
        assert_eq!(
            dispatch.reply_code().expect("Should be"),
            ReplyCode::Success(SuccessReplyReason::Auto)
        );
    });
}

#[test]
fn reply_deposit_to_program() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;

    // To program case.
    new_test_ext().execute_with(|| {
        let program_id = {
            let res = upload_program_default(USER_2, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            program_id.into(),
            0,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            10_000_000_000u64.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 2 init + 2 handle + 1 auto reply
        assert_total_dequeued(5);
        assert!(!MailboxOf::<Test>::is_empty(&checker));
    });
}

#[test]
fn reply_deposit_to_user_auto_reply() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;
    // To user case.
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            <[u8; 32]>::from(USER_2.into_origin()),
            0,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            10_000_000_000u64.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        // 1 init + 1 handle + 1 auto reply
        assert_total_dequeued(3);
        assert!(!MailboxOf::<Test>::is_empty(&checker));
    });
}

#[test]
fn reply_deposit_panic_in_handle_reply() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;

    // To user case with fail in handling reply.
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            <[u8; 32]>::from(USER_2.into_origin()),
            0,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            1u64.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        // 1 init + 1 handle + 1 auto reply
        assert_total_dequeued(3);
        assert!(MailboxOf::<Test>::is_empty(&checker));
    });
}

#[test]
fn reply_deposit_to_user_reply() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;

    // To user case.
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            <[u8; 32]>::from(USER_2.into_origin()),
            15_000,
        ));

        let reply_deposit = 10_000_000_000u64;

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            reply_deposit.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 1 init + 1 handle
        assert_total_dequeued(2);

        let mail = get_last_mail(USER_2);
        assert_eq!(
            mail.payload_bytes(),
            demo_reply_deposit::DESTINATION_MESSAGE
        );

        let user_2_balance = Balances::total_balance(&USER_2);
        assert_balance(USER_2, user_2_balance, 0u128);

        let value = 12_345u128;

        let reply_id = MessageId::generate_reply(mail.id());

        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_2),
            mail.id(),
            vec![],
            BlockGasLimitOf::<Test>::get(),
            value,
            false,
        ));

        assert_eq!(get_last_message_id(), reply_id);
        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        assert_balance(USER_2, user_2_balance - value, value);

        run_to_next_block(None);

        // 1 init + 1 handle + 1 reply
        assert_total_dequeued(3);
        assert!(!MailboxOf::<Test>::is_empty(&checker));
        assert_balance(USER_2, user_2_balance - value, 0u128);
    });
}

#[test]
fn reply_deposit_to_user_claim() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;

    // To user case.
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            <[u8; 32]>::from(USER_2.into_origin()),
            15_000,
        ));

        let reply_deposit = 10_000_000_000u64;

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            reply_deposit.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 1 init + 1 handle
        assert_total_dequeued(2);

        let mail = get_last_mail(USER_2);
        assert_eq!(
            mail.payload_bytes(),
            demo_reply_deposit::DESTINATION_MESSAGE
        );

        let user_2_balance = Balances::total_balance(&USER_2);
        assert_balance(USER_2, user_2_balance, 0u128);

        let reply_id = MessageId::generate_reply(mail.id());

        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        assert_ok!(Gear::claim_value(RuntimeOrigin::signed(USER_2), mail.id(),));

        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        assert_balance(USER_2, user_2_balance, 0u128);

        run_to_next_block(None);

        // 1 init + 1 handle + 1 auto reply on claim
        assert_total_dequeued(3);
        assert!(!MailboxOf::<Test>::is_empty(&checker));
        assert_balance(USER_2, user_2_balance, 0u128);
    });
}

#[test]
fn reply_deposit_to_user_out_of_rent() {
    use demo_constructor::demo_reply_deposit;

    init_logger();

    let checker = USER_1;

    // To user case.
    new_test_ext().execute_with(|| {
        let (_init_mid, constructor) = init_constructor(demo_reply_deposit::scheme(
            <[u8; 32]>::from(checker.into_origin()),
            <[u8; 32]>::from(USER_2.into_origin()),
            15_000,
        ));

        let reply_deposit = 10_000_000_000u64;

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            constructor,
            reply_deposit.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);
        // 1 init + 1 handle
        assert_total_dequeued(2);

        let (mail, interval) = MailboxOf::<Test>::iter_key(USER_2)
            .next()
            .expect("Element should be");

        assert_eq!(
            mail.payload_bytes(),
            demo_reply_deposit::DESTINATION_MESSAGE
        );

        let user_2_balance = Balances::total_balance(&USER_2);
        assert_balance(USER_2, user_2_balance, 0u128);

        let reply_id = MessageId::generate_reply(mail.id());

        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        // Hack to fast spend blocks till expiration.
        System::set_block_number(interval.finish - 1);
        Gear::set_block_number(interval.finish - 1);

        assert!(GasHandlerOf::<Test>::exists_and_deposit(reply_id));
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(reply_id).expect("Gas tree invalidated"),
            reply_deposit
        );

        assert_balance(USER_2, user_2_balance, 0u128);

        run_to_next_block(None);

        assert!(!GasHandlerOf::<Test>::exists(reply_id));

        // 1 init + 1 handle + 1 error reply on out of rent from mailbox
        assert_total_dequeued(3);
        assert!(!MailboxOf::<Test>::is_empty(&checker));
        assert_balance(USER_2, user_2_balance, 0u128);
    });
}

#[test]
fn reply_deposit_gstd_async() {
    use demo_waiting_proxy::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            (USER_2.into_origin().as_fixed_bytes(), 10_000_000_000u64).encode(),
            30_000_000_000,
            0,
            false,
        ));

        let program_id = get_last_program_id();

        let hello = b"Hello!";
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            hello.to_vec(),
            30_000_000_000,
            0,
            false,
        ));

        let handle_id = get_last_message_id();

        run_to_next_block(None);
        assert!(utils::is_active(program_id));

        let mail = get_last_mail(USER_2);
        assert_eq!(mail.payload_bytes(), hello);

        let hello_reply = b"U2";
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_2),
            mail.id(),
            hello_reply.to_vec(),
            0,
            0,
            false,
        ));

        run_to_next_block(None);

        assert_succeed(handle_id);

        let reply = maybe_any_last_message().expect("Should be");
        let (mid, code): (MessageId, ReplyCode) = reply.details().expect("Should be").into_parts();
        assert_eq!(mid, handle_id);
        assert_eq!(code, ReplyCode::Success(SuccessReplyReason::Manual));
        assert_eq!(reply.payload_bytes(), hello_reply);
    });
}

#[test]
fn pseudo_duplicate_wake() {
    use demo_constructor::{Calls, Scheme};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_msg_id, constructor) = init_constructor(Scheme::empty());

        let execute = |calls: Calls| {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                constructor,
                calls.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));
            let msg_id = get_last_message_id();
            run_to_next_block(None);

            msg_id
        };

        // message wakes some message id and waits
        let waited_msg_id = execute(Calls::builder().wake([0u8; 32]).wait());

        assert_last_dequeued(1);
        assert!(WaitlistOf::<Test>::contains(&constructor, &waited_msg_id));

        // message B wakes message A
        // message A results in waiting again
        execute(Calls::builder().wake(<[u8; 32]>::from(waited_msg_id)));

        assert_last_dequeued(2);
        assert!(WaitlistOf::<Test>::contains(&constructor, &waited_msg_id));
    });
}

#[test]
fn gasfull_after_gasless() {
    init_logger();

    let wat = format!(
        r#"
        (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_reply_wgas" (func $reply_wgas (param i32 i32 i64 i32 i32)))
        (import "env" "gr_send" (func $send (param i32 i32 i32 i32 i32)))
        (export "init" (func $init))
        (func $init
            i32.const 111 ;; ptr
            i32.const 1 ;; value
            i32.store

            (call $send (i32.const 111) (i32.const 0) (i32.const 32) (i32.const 10) (i32.const 333))
            (call $reply_wgas (i32.const 0) (i32.const 32) (i64.const {gas_limit}) (i32.const 222) (i32.const 333))
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
            false,
        ));

        // Make sure nothing panics.
        run_to_next_block(None);
    })
}

#[test]
fn backend_errors_handled_in_program() {
    use demo_custom::{InitMessage, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitMessage::BackendError.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
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
            1000, // value required in init function
            false,
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
            false,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);
        let mid_in_mailbox = utils::get_last_message_id();

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            mid_in_mailbox,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));

        run_to_next_block(None);
        assert!(Gear::is_exited(program_id));

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
            false,
        ));

        let program_id = utils::get_last_program_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            0,
            0,
            false,
        ));

        run_to_next_block(None);
        assert!(Gear::is_terminated(program_id));

        // Nothing panics here.
        assert_total_dequeued(2);
    })
}

#[test]
fn exited_program_zero_gas_and_value() {
    use crate::{Fortitude, Preservation, fungible};

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
        let ed = get_ed();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));

        let program_id = utils::get_last_program_id();

        // Account exists for the program and has exactly the ED locked.
        assert_program_balance(program_id, 0_u128, ed, 0_u128);
        // Reducible balance of an active program doesn't include the ED (runtime guarantee)
        assert_eq!(
            <CurrencyOf<Test> as fungible::Inspect<_>>::reducible_balance(
                &program_id.cast(),
                Preservation::Expendable,
                Fortitude::Polite,
            ),
            0
        );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            0,
            0,
            false,
        ));

        run_to_next_block(None);
        assert!(Gear::is_exited(program_id));

        // Nothing panics here.
        assert_total_dequeued(2);

        // Program's account should have been completely drained.
        assert_program_balance(program_id, 0_u128, 0_u128, 0_u128);
    })
}

#[test]
fn delayed_user_replacement() {
    use demo_constructor::demo_proxy_with_gas;

    fn scenario(gas_limit_to_forward: u64, to_mailbox: bool) {
        let code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
        let future_program_address =
            ActorId::generate_from_user(CodeId::generate(&code), DEFAULT_SALT);

        let (_init_mid, proxy) = init_constructor(demo_proxy_with_gas::scheme(
            future_program_address.into(),
            2,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            gas_limit_to_forward.encode(), // to be forwarded as gas limit
            gas_limit_to_forward + DEFAULT_GAS_LIMIT * 100,
            100_000_000, // before fix to be forwarded as value
            false,
        ));

        let message_id = utils::get_last_message_id();
        let delayed_id = MessageId::generate_outgoing(message_id, 0);

        run_to_block(3, None);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));

        assert_eq!(future_program_address, utils::get_last_program_id());

        run_to_block(4, None);

        // Message sending delayed.
        assert!(TaskPoolOf::<Test>::contains(
            &5,
            &ScheduledTask::SendUserMessage {
                message_id: delayed_id,
                to_mailbox
            }
        ));

        System::reset_events();

        run_to_next_block(None);
        assert!(Gear::is_initialized(future_program_address));

        // Delayed message sent.
        assert!(!TaskPoolOf::<Test>::contains(
            &5,
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
    use demo_constructor::demo_proxy_with_gas;

    // Testing that correct gas amount will be reserved and paid for holding.
    fn scenario(delay: BlockNumber) {
        // Upload program that sends message to any user.
        let (_init_mid, proxy) = init_constructor(demo_proxy_with_gas::scheme(
            USER_2.into_origin().into(),
            delay.saturated_into(),
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));

        let proxy_msg_id = get_last_message_id();
        let balance_rent_pool = Balances::free_balance(RENT_POOL);

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash().saturating_mul(
                delay
                    .saturating_add(CostsPerBlockOf::<Test>::reserve_for())
                    .saturated_into(),
            ),
        );

        let reserve_for_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash()
                .saturating_mul(CostsPerBlockOf::<Test>::reserve_for().saturated_into()),
        );

        // Gas should be reserved while message is being held in storage.
        assert_eq!(GearBank::<Test>::account_total(&USER_1), delay_holding_fee);
        let total_balance =
            Balances::free_balance(USER_1) + GearBank::<Test>::account_total(&USER_1);

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
        let message = maybe_any_last_message().expect("Should be");
        assert_eq!(delayed_id, message.id());

        // Mailbox should be empty.
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        // Check balances match and gas charging is correct.
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);
        assert_eq!(
            total_balance - delay_holding_fee + reserve_for_fee,
            Balances::free_balance(USER_1)
        );
        assert_eq!(
            Balances::free_balance(RENT_POOL),
            balance_rent_pool + delay_holding_fee - reserve_for_fee
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

    // Testing that correct gas amount will be reserved and paid for holding.
    fn scenario(delay: BlockNumber) {
        let reservation_amount = 6_000_000_000u64;

        // Upload program that sends message to any user.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PROXY_WGAS_WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: USER_2.cast(),
                delay: delay.saturated_into(),
                reservation_amount,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
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
            false,
        ));

        let proxy_msg_id = get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash().saturating_mul(
                delay
                    .saturating_add(CostsPerBlockOf::<Test>::reserve_for())
                    .saturated_into(),
            ),
        );

        let reserve_for_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash()
                .saturating_mul(CostsPerBlockOf::<Test>::reserve_for().saturated_into()),
        );

        let mailbox_gas_threshold = gas_price(<Test as Config>::MailboxThreshold::get());

        // At this point a `Cut` node has been created with `mailbox_threshold` as value and
        // `delay` + 1 locked for using dispatch stash storage.
        // Other gas nodes have been consumed with all gas released to the user.
        assert_eq!(
            GearBank::<Test>::account_total(&USER_1),
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

        let last_mail = get_last_mail(USER_2);
        assert_eq!(last_mail.id(), delayed_id);

        // Mailbox should not be empty.
        assert!(!MailboxOf::<Test>::is_empty(&USER_2));

        // At this point the `Cut` node has all its value locked for using mailbox storage.
        // The extra `reserve_for_fee` as a leftover from the message having been charged exactly
        // for the `delay` number of blocks spent in the dispatch stash so that the "+ 1" security
        // margin remained unused and was simply added back to the `Cut` node value.
        assert_eq!(
            GearBank::<Test>::account_total(&USER_1),
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
    use demo_constructor::demo_proxy_with_gas;

    // Testing that correct gas amount will be reserved and paid for holding.
    fn scenario(delay: BlockNumber) {
        // Upload empty program that receive the message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));

        let program_address = utils::get_last_program_id();

        // Upload program that sends message to another program.
        let (_init_mid, proxy) = init_constructor(demo_proxy_with_gas::scheme(
            program_address.into(),
            delay.saturated_into(),
        ));
        assert!(Gear::is_initialized(program_address));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            0u64.encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
        ));
        let proxy_msg_id = utils::get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash().saturating_mul(
                delay
                    .saturating_add(CostsPerBlockOf::<Test>::reserve_for())
                    .saturated_into(),
            ),
        );

        let reserve_for_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash()
                .saturating_mul(CostsPerBlockOf::<Test>::reserve_for().saturated_into()),
        );

        // Gas should be reserved while message is being held in storage.
        assert_eq!(GearBank::<Test>::account_total(&USER_1), delay_holding_fee);
        let total_balance =
            Balances::free_balance(USER_1) + GearBank::<Test>::account_total(&USER_1);

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
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);
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

    // Testing that correct gas amount will be reserved and paid for holding.
    fn scenario(delay: BlockNumber) {
        // Upload empty program that receive the message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
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
                delay: delay.saturated_into(),
                reservation_amount,
            }
            .encode(),
            DEFAULT_GAS_LIMIT * 100,
            0,
            false,
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
            false,
        ));
        let proxy_msg_id = utils::get_last_message_id();

        // Run blocks to make message get into dispatch stash.
        run_to_block(3, None);

        let delay_holding_fee = gas_price(
            CostsPerBlockOf::<Test>::dispatch_stash().saturating_mul(
                delay
                    .saturating_add(CostsPerBlockOf::<Test>::reserve_for())
                    .saturated_into(),
            ),
        );

        let reservation_holding_fee = gas_price(
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
        let gas_locked_in_gas_node =
            gas_price(Gas::get_lock(delayed_id, LockId::DispatchStash).unwrap());
        assert_eq!(gas_locked_in_gas_node, delay_holding_fee);

        // Gas should be reserved while message is being held in storage.
        assert_eq!(
            GearBank::<Test>::account_total(&USER_1),
            gas_price(reservation_amount) + reservation_holding_fee
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

        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);
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
            2 * get_ed(), // necessary for init function to succeed
            false,
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
        let reserved_balance = GearBank::<Test>::account_total(&USER_1);

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
        // One db read burned for querying program data from storage when creating program,
        // and one more to process error reply.
        assert_last_dequeued(2);

        let delayed_block_amount: u64 = 1;
        let delay_holding_fee = gas_price(
            delayed_block_amount.saturating_mul(CostsPerBlockOf::<Test>::dispatch_stash()),
        );
        let read_program_from_storage_fee =
            gas_price(DbWeightOf::<Test>::get().reads(1).ref_time());
        let read_code_metadata_from_storage_fee =
            gas_price(DbWeightOf::<Test>::get().reads(1).ref_time());

        assert_eq!(
            Balances::free_balance(USER_1),
            free_balance + reserved_balance
                - delay_holding_fee
                - 2 * read_program_from_storage_fee
                - read_code_metadata_from_storage_fee
        );
        assert!(GearBank::<Test>::account_total(&USER_1).is_zero());
    })
}

#[test]
fn unstoppable_block_execution_works() {
    init_logger();

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let user_balance = Balances::free_balance(USER_1);

        // This manipulations are required due to we have only gas to value conversion.
        let executions_amount = 100_u64;
        let gas_for_each_execution = BlockGasLimitOf::<Test>::get();

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
                false,
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
            user_balance - gas_price(real_gas_to_burn)
        );
    })
}

#[test]
fn read_state_works() {
    use demo_fungible_token::{InitConfig, IoFungibleToken, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitConfig::test_sequence().encode(),
            DEFAULT_GAS_LIMIT * 100,
            10_000,
            false,
        ));

        let program_id = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));

        let expected = IoFungibleToken::test_sequence().encode();

        let res = Gear::read_state_impl(program_id, Default::default(), None)
            .expect("Failed to read state");

        assert_eq!(res, expected);
    });
}

#[test]
fn mailbox_rent_out_of_rent() {
    use demo_constructor::{Scheme, demo_value_sender::TestData};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, sender) = init_constructor_with_value(Scheme::empty(), 10_000);

        // Message removes due to out of rent condition.
        //
        // For both cases value moves back to program.
        let cases = [
            // Gasful message.
            TestData::gasful(<Test as Config>::MailboxThreshold::get() * 2, 1_000),
            // Gasless message.
            TestData::gasless(3_000, <Test as Config>::MailboxThreshold::get()),
        ];

        let mb_cost = CostsPerBlockOf::<Test>::mailbox();
        let reserve_for = CostsPerBlockOf::<Test>::reserve_for();

        for data in cases {
            let user_1_balance = Balances::free_balance(USER_1);
            assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

            let user_2_balance = Balances::free_balance(USER_2);
            assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);

            let prog_balance = Balances::free_balance(sender.cast::<AccountId>());
            assert_eq!(GearBank::<Test>::account_total(&sender.cast()), 0);

            let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
                USER_1,
                sender,
                data.request(USER_2.into_origin()).encode(),
                Some(data.extra_gas),
                0,
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_info.min_limit + data.extra_gas),
                gas_price(gas_info.min_limit + data.extra_gas),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            let hold_bound = HoldBoundBuilder::<Test>::new(StorageType::Mailbox)
                .maximum_for(data.gas_limit_to_send);

            let expected_duration =
                BlockNumberFor::<Test>::saturated_from(data.gas_limit_to_send / mb_cost)
                    - reserve_for;

            assert_eq!(hold_bound.expected_duration(), expected_duration);

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_info.burned + data.gas_limit_to_send),
                gas_price(data.gas_limit_to_send),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, data.value);
            assert!(!MailboxOf::<Test>::is_empty(&USER_2));

            run_to_block(hold_bound.expected(), None);

            let gas_totally_burned = gas_info.burned + data.gas_limit_to_send
                - GasBalanceOf::<Test>::saturated_from(reserve_for) * mb_cost;

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_totally_burned),
                0u128,
            );
            utils::assert_balance(USER_2, user_2_balance + data.value, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            // auto generated reply on out of rent from mailbox
            assert_last_dequeued(1);
        }
    });
}

#[test]
fn mailbox_rent_claimed() {
    use demo_constructor::{Scheme, demo_value_sender::TestData};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, sender) = init_constructor_with_value(Scheme::empty(), 10_000);

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
            assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

            let user_2_balance = Balances::free_balance(USER_2);
            assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);
            let prog_balance = Balances::free_balance(sender.cast::<AccountId>());
            assert_eq!(GearBank::<Test>::account_total(&sender.cast()), 0);

            let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
                USER_1,
                sender.cast(),
                data.request(USER_2.into_origin()).encode(),
                Some(data.extra_gas),
                0,
            );

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_info.min_limit + data.extra_gas),
                gas_price(gas_info.min_limit + data.extra_gas),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance, 0u128);
            assert!(MailboxOf::<Test>::is_empty(&USER_2));

            run_to_next_block(None);

            let message_id = utils::get_last_message_id();

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_info.burned + data.gas_limit_to_send),
                gas_price(data.gas_limit_to_send),
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
                user_1_balance - gas_price(gas_info.burned + data.gas_limit_to_send),
                gas_price(data.gas_limit_to_send),
            );
            utils::assert_balance(USER_2, user_2_balance, 0u128);
            utils::assert_balance(sender, prog_balance - data.value, data.value);
            assert!(!MailboxOf::<Test>::is_empty(&USER_2));

            assert_ok!(Gear::claim_value(RuntimeOrigin::signed(USER_2), message_id));

            utils::assert_balance(
                USER_1,
                user_1_balance - gas_price(gas_info.burned + duration * mb_cost),
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
    use demo_constructor::{Scheme, demo_value_sender::TestData};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, sender) = init_constructor_with_value(Scheme::empty(), 10_000);

        // Message with 0 gas limit is not added to mailbox.
        let gas_limit = 0;
        let value = 1_000;

        let user_1_balance = Balances::free_balance(USER_1);
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

        let user_2_balance = Balances::free_balance(USER_2);
        assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);

        let prog_balance = Balances::free_balance(sender.cast::<AccountId>());
        assert_eq!(GearBank::<Test>::account_total(&sender.cast()), 0);

        let payload = TestData::gasful(gas_limit, value);

        // Used like that, because calculate gas info always provides
        // message into mailbox while sending without gas.
        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(sender),
            payload.request(USER_2.into_origin()).encode(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            sender,
            payload.request(USER_2.into_origin()).encode(),
            gas_info.burned + gas_limit,
            0,
            false,
        ));

        utils::assert_balance(
            USER_1,
            user_1_balance - gas_price(gas_info.burned + gas_limit),
            gas_price(gas_info.burned + gas_limit),
        );
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        run_to_next_block(None);

        utils::assert_balance(USER_1, user_1_balance - gas_price(gas_info.burned), 0u128);
        utils::assert_balance(USER_2, user_2_balance + value, 0u128);
        utils::assert_balance(sender, prog_balance - value, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));
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
                balance + 1,
                false,
            ),
            pallet_gear_bank::Error::<Test>::InsufficientBalance
        );

        assert_noop!(
            upload_program_default(LOW_BALANCE_USER, ProgramCodeKind::Default),
            pallet_gear_bank::Error::<Test>::InsufficientBalance
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
                0,
                false,
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
        let ed = get_ed();

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
        let user1_potential_msgs_spends = gas_price(2 * DEFAULT_GAS_LIMIT);
        // User 1 has sent two messages
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends - ed
        );

        // Clear messages from the queue to refund unused gas
        run_to_block(2, None);

        // Checking that sending a message to a non-program address works as a value transfer
        let mail_value = 20_000;

        // Take note of up-to-date users balance
        let user1_initial_balance = Balances::free_balance(USER_1);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            USER_2.cast(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            mail_value,
            false,
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
    use demo_constructor::demo_proxy_with_gas;

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, proxy) =
            init_constructor(demo_proxy_with_gas::scheme(USER_1.into_origin().into(), 0));

        let rent = <Test as Config>::MailboxThreshold::get();

        let check_result = |sufficient: bool| -> MessageId {
            run_to_next_block(None);

            let mailbox_key = USER_1.cast();
            let message_id = get_last_message_id();

            if sufficient {
                // * message has been inserted into the mailbox.
                // * the ValueNode has been created.
                assert!(MailboxOf::<Test>::contains(&mailbox_key, &message_id));
                // All gas in the gas node has been locked
                assert_ok!(GasHandlerOf::<Test>::get_limit(message_id), 0);
                assert_ok!(
                    GasHandlerOf::<Test>::get_lock(message_id, LockId::Mailbox),
                    rent
                );
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
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        check_result(false);

        // send message with enough gas_limit
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            (rent).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        let message_id = check_result(true);

        // send reply with enough gas_limit
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            rent.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        let message_id = check_result(true);

        // send reply with insufficient message rent
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            (rent - 1).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));
        check_result(false);
    });
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        assert!(utils::is_active(program_id));
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
            pallet_gear_bank::Error::<Test>::InsufficientBalance
        );

        let low_balance_user_balance = Balances::free_balance(LOW_BALANCE_USER);
        let user_1_balance = Balances::free_balance(USER_1);
        let value = 1000;

        // Because destination is user, no gas will be reserved
        MailboxOf::<Test>::clear();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(LOW_BALANCE_USER),
            USER_1.cast(),
            EMPTY_PAYLOAD.to_vec(),
            10,
            value,
            false,
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
                0,
                false,
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

        assert_ok!(send_default_message(USER_1, USER_2.cast()));
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
        let gas_spent = gas_price(
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
        let ed = get_ed();

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
            0,
            false,
        ));

        // Spends for submit program with default gas limit and sending default message with a huge gas limit.
        // Existential deposit has been charged to create an account for the submitted program.
        let user1_potential_msgs_spends =
            gas_price(DEFAULT_GAS_LIMIT + huge_send_message_gas_limit);

        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends - ed
        );
        assert_eq!(
            GearBank::<Test>::account_total(&USER_1),
            user1_potential_msgs_spends
        );

        run_to_block(2, None);

        let user1_actual_msgs_spends = gas_price(
            BlockGasLimitOf::<Test>::get()
                .saturating_sub(GasAllowanceOf::<Test>::get())
                .saturating_sub(minimal_weight.ref_time()),
        );

        assert!(user1_potential_msgs_spends > user1_actual_msgs_spends);

        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_actual_msgs_spends - ed
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
            false,
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
            false,
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
            false,
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
            false,
        );
        assert_ok!(res);

        run_to_block(4, None);
        assert_last_dequeued(1);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));
    });
}

#[test]
fn gas_limit_exceeded_oob_case() {
    let wat = r#"(module
        (import "env" "memory" (memory 512))
        (import "env" "gr_send_init" (func $send_init (param i32)))
        (import "env" "gr_send_push" (func $send_push (param i32 i32 i32 i32)))
        (export "init" (func $init))
        (func $init
            (local $addr i32)
            (local $handle i32)

            ;; init message sending
            i32.const 0x0
            call $send_init

            ;; load handle and set it to local
            i32.const 0x0
            i32.load
            local.set $handle

            ;; push message payload out of bounds
            ;; each iteration we change gear page where error is returned
            (loop
                local.get $handle
                i32.const 0x1000_0000 ;; out of bounds payload addr
                i32.const 0x1
                local.get $addr
                call $send_push

                local.get $addr
                i32.const 0x4000
                i32.add
                local.tee $addr
                i32.const 0x0200_0000
                i32.ne
                br_if 0
            )
        )
    )"#;

    init_logger();
    new_test_ext().execute_with(|| {
        let gas_limit = 10_000_000_000;
        let code = ProgramCodeKind::Custom(wat).to_bytes();
        let salt = DEFAULT_SALT.to_vec();
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            gas_limit,
            0,
            false,
        )
        .unwrap();

        let message_id = get_last_message_id();

        run_to_block(2, None);
        assert_last_dequeued(1);

        // We have sent message with `gas_limit`, but it must not be enough,
        // because one write access to memory costs 100_000_000 gas (storage write cost).
        // Fallible syscall error is written in each iteration to new gear page,
        // so to successfully finish execution must be at least 100_000_000 * 512 * 4 = 204_800_000_000 gas,
        // which is bigger than provided `gas_limit`.
        assert_failed(
            message_id,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );
    });
}

#[test]
fn lazy_pages() {
    use gear_core::pages::GearPage;
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
                false,
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
            false,
        );
        assert_ok!(res);

        run_to_block(3, None);

        // Dirty hack: lazy pages info is stored in thread local static variables,
        // so after program execution lazy-pages information
        // remains correct and we can use it here.
        let write_accessed_pages: BTreeSet<_> = gear_ri::gear_ri::write_accessed_pages()
            .into_iter()
            .collect();

        // checks accessed pages set
        let mut expected_write_accessed_pages = BTreeSet::new();

        // released from 0 wasm page:
        expected_write_accessed_pages.insert(0);

        // released from 2 wasm page:
        expected_write_accessed_pages.insert(0x23ffe / GearPage::SIZE);
        expected_write_accessed_pages.insert(0x24001 / GearPage::SIZE);

        // nothing for 5 wasm page, because it's just read access

        // released from 8 and 9 wasm pages, must be several gear pages:
        expected_write_accessed_pages.insert(0x8fffc / GearPage::SIZE);
        expected_write_accessed_pages.insert(0x90003 / GearPage::SIZE);

        assert_eq!(write_accessed_pages, expected_write_accessed_pages);
    });
}

#[test]
fn initial_pages_cheaper_than_allocated_pages() {
    // When program has some amount of the initial pages, then it is simpler
    // for core processor and executor than process the same program
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
                false,
            );
            assert_ok!(res);

            run_to_next_block(None);
            assert_last_dequeued(1);

            gas_price(BlockGasLimitOf::<Test>::get().saturating_sub(GasAllowanceOf::<Test>::get()))
        };

        let spent_for_initial_pages = gas_spent(wat_initial);
        let spent_for_allocated_pages = gas_spent(wat_alloc);
        assert!(
            spent_for_initial_pages < spent_for_allocated_pages,
            "spent {spent_for_initial_pages} gas for initial pages, spent {spent_for_allocated_pages} gas for allocated pages",
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
            1000,
            false,
        ));
        let failed1 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid1,
            EMPTY_PAYLOAD.to_vec(),
            gas1.min_limit,
            1000,
            false,
        ));
        let succeed1 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            gas2.min_limit - 1,
            1000,
            false,
        ));
        let failed2 = get_last_message_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            EMPTY_PAYLOAD.to_vec(),
            gas2.min_limit,
            1000,
            false,
        ));
        let succeed2 = get_last_message_id();

        run_to_next_block(None);

        assert_last_dequeued(4);
        assert_succeed(succeed1);
        assert_succeed(succeed2);

        assert_failed(
            failed1,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );

        assert_failed(
            failed2,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );

        // =========== BLOCK 4 ============

        let (gas1, gas2) = calc_gas();

        let send_with_min_limit_to = |pid: ActorId, gas: &GasInfo| {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                EMPTY_PAYLOAD.to_vec(),
                gas.min_limit,
                1000,
                false,
            ));
        };

        send_with_min_limit_to(pid1, &gas1);
        send_with_min_limit_to(pid2, &gas2);

        assert!(gas1.burned + gas2.burned < gas1.min_limit + gas2.min_limit);

        // program1 sends message to a user and it goes to the TaskPool
        let weight = minimal_weight + tasks_add_weight;
        // both processed if gas allowance equals only burned count
        run_to_next_block(Some(weight.ref_time() + gas1.burned + gas2.burned + 1));
        assert_last_dequeued(2);
        assert_eq!(GasAllowanceOf::<Test>::get(), 1);

        // =========== BLOCK 5 ============
        let (gas1, gas2) = calc_gas();
        // Check that gas allowance has not changed after calc_gas execution
        assert_eq!(GasAllowanceOf::<Test>::get(), 1);

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
            GearBank::<Test>::account_total(&USER_1),
            gas_price(OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS)
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
                Some(ErrorReplyReason::Execution(
                    SimpleExecutionError::RanOutOfGas,
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
        assert!(utils::is_active(program_id));

        run_to_block(2, None);

        assert!(Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        // Submitting second program, which fails on initialization, therefore is deleted
        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::GreedyInit);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        run_to_block(3, None);

        assert!(!Gear::is_initialized(program_id));
        // while at the same time is terminated
        assert!(!utils::is_active(program_id));
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
        let mut next_block = 2u64;

        let tests: [(_, _, Option<ErrorReplyReason>); 5] = [
            // Code, init failure reason, handle succeed flag
            (ProgramCodeKind::Default, None, None),
            (
                ProgramCodeKind::GreedyInit,
                Some(ErrorReplyReason::Execution(
                    SimpleExecutionError::RanOutOfGas,
                )),
                None,
            ),
            (
                ProgramCodeKind::Custom(wat_trap_in_init),
                Some(ErrorReplyReason::Execution(
                    SimpleExecutionError::UnreachableInstruction,
                )),
                None,
            ),
            // First try asserts by status code.
            (
                ProgramCodeKind::Custom(wat_trap_in_handle),
                None,
                Some(ErrorReplyReason::Execution(
                    SimpleExecutionError::UnreachableInstruction,
                )),
            ),
            // Second similar try asserts by error payload explanation.
            (
                ProgramCodeKind::Custom(wat_trap_in_handle),
                None,
                Some(ErrorReplyReason::Execution(
                    SimpleExecutionError::UnreachableInstruction,
                )),
            ),
        ];

        for (code_kind, init_failure_reason, handle_failure_reason) in tests {
            System::reset_events();
            let program_id = {
                let res = upload_program_default_with_salt(
                    USER_1,
                    next_block.to_le_bytes().to_vec(),
                    code_kind,
                );
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
            &prog_id.cast(),
            2000,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000,
            1000, // `prog_id` sent message with value of 1000 (see program code)
            false,
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
                5.cast(), // non existent `reply_to_id`
                EMPTY_PAYLOAD.to_vec(),
                DEFAULT_GAS_LIMIT,
                0,
                false,
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
            &prog_id.cast(),
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
            assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

            assert!(MailboxOf::<Test>::contains(&USER_1, &reply_to_id));

            assert_eq!(
                GearBank::<Test>::account_total(&USER_2),
                gas_price(OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS)
            );

            // nothing changed
            assert_eq!(Balances::free_balance(USER_1), user_balance);
            assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

            // auto-claim of "locked_value" + send is here
            assert_ok!(Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                reply_to_id,
                EMPTY_PAYLOAD.to_vec(),
                gas_limit_to_reply,
                value_to_reply,
                false,
            ));

            let currently_sent = value_to_reply + gas_price(gas_limit_to_reply);

            assert_eq!(
                Balances::free_balance(USER_1),
                user_balance + locked_value - currently_sent
            );
            assert_eq!(GearBank::<Test>::account_total(&USER_1), currently_sent);
            assert_eq!(GearBank::<Test>::account_total(&USER_2), 0,);
        }
    })
}

// user 1 sends to prog msg
// prog send to user 1 msg to mailbox
// user 1 claims it from mailbox -> goes auto-reply
#[test]
fn claim_value_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let sender_balance = Balances::free_balance(USER_2);
        assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);
        let claimer_balance = Balances::free_balance(USER_1);
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

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
            true,
        )
        .expect("calculate_gas_info failed");

        let gas_burned = gas_price(gas_burned - may_be_returned);

        run_to_block(bn_of_insertion + holding_duration, None);

        let balance_rent_pool = Balances::free_balance(RENT_POOL);

        assert_ok!(Gear::claim_value(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
        ));

        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);
        assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);

        let expected_claimer_balance = claimer_balance + value_sent;
        assert_eq!(Balances::free_balance(USER_1), expected_claimer_balance);

        let burned_for_hold = gas_price(
            GasBalanceOf::<Test>::saturated_from(holding_duration)
                * CostsPerBlockOf::<Test>::mailbox(),
        );

        // In `calculate_gas_info` program start to work with page data in storage,
        // so need to take in account gas, which spent for data loading.
        let charged_for_page_load = gas_price(
            <Test as Config>::Schedule::get()
                .memory_weights
                .load_page_data
                .ref_time(),
        );

        // Gas left returns to sender from consuming of value tree while claiming.
        let expected_sender_balance =
            sender_balance + charged_for_page_load - value_sent - gas_burned - burned_for_hold;
        assert_eq!(Balances::free_balance(USER_2), expected_sender_balance);

        // To trigger GearBank::on_finalize -> transfer to pool performed.
        run_to_next_block(Some(0));

        assert_eq!(
            Balances::free_balance(RENT_POOL),
            balance_rent_pool + burned_for_hold
        );

        System::assert_has_event(
            Event::UserMessageRead {
                id: reply_to_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            }
            .into(),
        );

        run_to_next_block(None);

        // Init + handle + auto-reply on claim
        assert_total_dequeued(3);
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
            0u128,
            false,
        ));

        let init_message_id = utils::get_last_message_id();
        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        run_to_block(2, None);

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));
        assert!(WaitlistOf::<Test>::contains(&program_id, &init_message_id));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(1),
            program_id,
            vec![],
            0, // that triggers unreachable code atm
            0,
            false,
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
            false,
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
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            Request::Receive(10).encode(),
            30_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        // We sent two messages to user
        assert_eq!(utils::user_messages_sent(), (2, 0));

        // Despite some messages are still in the mailbox and the program still being active
        // (therefore, holding the existential deposit), all gas locked in value trees
        // has been refunded to the sender so the free balances should add up
        let final_balance = Balances::free_balance(USER_1)
            + Balances::free_balance(BLOCK_AUTHOR)
            + Balances::free_balance(program_id.cast::<AccountId>());

        assert_eq!(initial_balance, final_balance);

        // All gas cancelled out in the end
        assert!(GasHandlerOf::<Test>::total_supply().is_zero());
    });
}

#[test]
fn test_code_submission_pass() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_id = CodeId::generate(&code);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone()
        ));

        let saved_code = <Test as Config>::CodeStorage::get_instrumented_code(code_id);

        let schedule = <Test as Config>::Schedule::get();
        let code = Code::try_new(
            code,
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            schedule.limits.stack_height,
            schedule.limits.data_segments_amount.into(),
        )
        .expect("Error creating Code");
        assert_eq!(
            saved_code.unwrap().bytes(),
            code.instrumented_code().bytes()
        );

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
        let code_id = CodeId::generate(&code);

        // First submit program, which will set code and metadata
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
            false,
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
        assert!(<Test as Config>::CodeStorage::original_code_exists(code_id));

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

        // First submit code
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            code.clone()
        ));
        let expected_code_saved_events = 1;

        // Submit program from another origin. Should not change meta or code.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            code,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
            false,
        ));

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
            0u128,
            false,
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        run_to_block(2, None);

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            10_000u64,
            0u128,
            false,
        ));

        run_to_block(3, None);

        let auto_reply = maybe_last_message(USER_1).expect("Should be");
        assert!(auto_reply.details().is_some());
        assert_eq!(
            auto_reply.reply_code().expect("Should be"),
            ReplyCode::Error(ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::Uninitialized
            ))
        );
        assert_eq!(auto_reply.payload_bytes(), &[] as &[u8]);
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
            0u128,
            false,
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

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
            false,
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
            0u128,
            false,
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
            false,
        ));

        run_to_block(3, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            10_000_000_000u64,
            0u128,
            false,
        ));

        run_to_block(4, None);

        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(
            maybe_last_message(USER_1)
                .expect("Event should be")
                .payload_bytes(),
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
            0u128,
            false,
        ));

        let program_id = utils::get_last_program_id();

        run_to_block(2, None);

        // While program is not inited all messages addressed to it got error reply
        let n = 10;
        for _ in 0..n {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_3),
                program_id,
                vec![],
                5_000_000_000u64,
                0u128,
                false,
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
            false,
        ));

        run_to_block(20, None);

        let actual_n = System::events()
            .into_iter()
            .filter_map(|e| match e.event {
                MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. })
                    if message.destination().into_origin() == USER_3.into_origin() =>
                {
                    assert_eq!(
                        message.reply_code(),
                        Some(ReplyCode::Error(ErrorReplyReason::UnavailableActor(
                            SimpleUnavailableActorError::Uninitialized
                        )))
                    );
                    Some(())
                }
                _ => None,
            })
            .count();

        assert_eq!(actual_n, n);
    })
}

#[test]
fn test_different_waits_success() {
    use demo_waiter::{Command, WASM_BINARY, WaitSubcommand};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000u64,
            0u128,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(program_id));

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
        let payload = Command::Wait(WaitSubcommand::Wait).encode();
        let duration = 5;
        let wl_gas = duration_gas(duration) + reserve_gas;
        let value = 0;
        let balance_rent_pool = Balances::free_balance(RENT_POOL);

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_success = get_last_message_id();

        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_success), expiration(duration));

        run_for_blocks(duration.into(), None);

        // rent for keeping the message in the wait list should go to the pool
        assert_eq!(
            Balances::free_balance(RENT_POOL),
            balance_rent_pool + gas_price(duration_gas(duration)),
        );

        // Command::WaitFor case.
        let duration = 5;
        let payload = Command::Wait(WaitSubcommand::WaitFor(duration)).encode();
        let wl_gas = duration_gas(duration) + reserve_gas + 100_000_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_for_success = get_last_message_id();

        run_to_next_block(None);

        assert_eq!(
            get_waitlist_expiration(wait_for_success),
            expiration(duration)
        );

        // Command::WaitUpTo case.
        let duration = 5;
        let payload = Command::Wait(WaitSubcommand::WaitUpTo(duration)).encode();
        let wl_gas = duration_gas(duration) + reserve_gas + 100_000_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
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
    use demo_waiter::{Command, WASM_BINARY, WaitSubcommand};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000u64,
            0u128,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(program_id));

        let system_reservation = demo_waiter::system_reserve();

        // Command::Wait case no gas.
        let payload = Command::Wait(WaitSubcommand::Wait).encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_gas,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );

        // Command::WaitFor case no gas.
        let payload = Command::Wait(WaitSubcommand::WaitFor(10)).encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_for_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_for_gas,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );

        // Command::WaitUpTo case no gas.
        let payload = Command::Wait(WaitSubcommand::WaitUpTo(10)).encode();
        let wl_gas = 0;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            payload.clone(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_up_to_gas = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_up_to_gas,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );

        // Command::WaitFor case invalid argument.
        let payload = Command::Wait(WaitSubcommand::WaitFor(0)).encode();
        let wl_gas = 10_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            // Hack to avoid calculating gas info fail.
            Command::Wait(WaitSubcommand::WaitFor(1)).encode(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_for_arg = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_for_arg,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );

        // Command::WaitUpTo case invalid argument.
        let payload = Command::Wait(WaitSubcommand::WaitUpTo(0)).encode();
        let wl_gas = 10_000;
        let value = 0;

        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(program_id),
            // Hack to avoid calculating gas info fail.
            Command::Wait(WaitSubcommand::WaitUpTo(1)).encode(),
            value,
            false,
            true,
        )
        .expect("calculate_gas_info failed");

        assert!(gas_info.waited);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            gas_info.burned + wl_gas + system_reservation,
            value,
            false,
        ));

        let wait_up_to_arg = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            wait_up_to_arg,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );
    });
}

#[test]
fn wait_after_reply() {
    use demo_waiter::{Command, WASM_BINARY, WaitSubcommand};

    let test = |subcommand: WaitSubcommand| {
        new_test_ext().execute_with(|| {
            log::debug!("{subcommand:?}");

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                WASM_BINARY.to_vec(),
                DEFAULT_SALT.to_vec(),
                EMPTY_PAYLOAD.to_vec(),
                100_000_000u64,
                0u128,
                false,
            ));

            let program_id = get_last_program_id();

            run_to_next_block(None);
            assert!(utils::is_active(program_id));

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                program_id,
                Command::ReplyAndWait(subcommand).encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));

            let message_id = utils::get_last_message_id();

            run_to_next_block(None);
            assert_failed(
                message_id,
                ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
            );
        });
    };

    init_logger();
    test(WaitSubcommand::Wait);
    test(WaitSubcommand::WaitFor(15));
    test(WaitSubcommand::WaitUpTo(15));
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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        let duration = 10;
        let payload = Command::SendAndWaitFor(duration, USER_1.into_origin().into()).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
        ));

        // Fast forward blocks.
        let message_id = get_last_message_id();
        run_to_next_block(None);
        let now = System::block_number();

        System::set_block_number(duration.saturated_into::<u64>() + now - 1);
        Gear::set_block_number(duration.saturated_into::<u64>() + now - 1);

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
        assert_eq!(get_last_mail(USER_1).payload_bytes(), b"ping");
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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        // Case 1 - `Command::SendFor`
        //
        // Send message and then wait_for.
        let duration = 5;
        let payload = Command::SendFor(USER_1.into_origin().into(), duration).encode();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            25_000_000_000,
            0,
            false,
        ));

        let wait_for = get_last_message_id();
        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_for), expiration(duration));

        // Case 2 - `Command::SendUpTo`
        //
        // Send message and then wait_up_to.
        let duration = 10;
        let payload = Command::SendUpTo(USER_1.into_origin().into(), duration).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            25_000_000_000,
            0,
            false,
        ));

        let wait_no_more = get_last_message_id();
        run_to_next_block(None);

        assert_eq!(get_waitlist_expiration(wait_no_more), expiration(duration));

        // Case 3 - `Command::SendUpToWait`
        //
        // Send message and then wait no_more, wake, wait no_more again.
        let duration = 10;
        let payload = Command::SendUpToWait(USER_2.into_origin().into(), duration).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
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
            false,
        ));

        run_to_next_block(None);

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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // `Command::SendTimeout`
        //
        // Emits error when locks are timeout
        let duration = 10u64;
        let payload = Command::SendTimeout(USER_1.cast(), duration.saturated_into()).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);
        let now = System::block_number();
        let target = duration + now - 1;

        // Try waking the processed message.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            Command::Wake.encode(),
            10_000_000,
            0,
            false,
        ));

        run_to_next_block(None);
        System::set_block_number(target);
        Gear::set_block_number(target);
        System::reset_events();
        run_to_next_block(None);

        // Timeout still works.
        assert!(
            MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes().to_vec() == b"timeout")
        );
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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // Join two waited messages, futures complete at
        // the same time when both of them are finished.
        let duration_a: BlockNumber = 5;
        let duration_b: BlockNumber = 10;
        let payload = Command::JoinTimeout(
            USER_1.cast(),
            duration_a.saturated_into(),
            duration_b.saturated_into(),
        )
        .encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        // Run to each of the targets and check if we can get the timeout result.
        let now = System::block_number();
        let targets = [duration_a, duration_b].map(|target| target + now - 1);
        let run_to_target = |target: BlockNumber| {
            System::set_block_number(target);
            Gear::set_block_number(target);
            run_to_next_block(None);
        };

        // Run to the end of the first duration.
        //
        // The timeout message has not been triggered yet.
        run_to_target(targets[0]);
        assert!(
            !MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes().to_vec() == b"timeout")
        );

        // Run to the end of the second duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[1]);
        assert!(
            MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes().to_vec() == b"timeout")
        );
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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        // Select from two waited messages, futures complete at
        // the same time when one of them getting failed.
        let duration_a: BlockNumber = 5;
        let duration_b: BlockNumber = 10;
        let payload = Command::SelectTimeout(
            USER_1.cast(),
            duration_a.saturated_into(),
            duration_b.saturated_into(),
        )
        .encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        // Run to the end of the first duration.
        //
        // The timeout message has been triggered.
        let now = System::block_number();
        let target = duration_a + now - 1;
        System::set_block_number(target);
        Gear::set_block_number(target);
        run_to_next_block(None);

        assert!(
            MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes().to_vec() == b"timeout")
        );
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
            0u128,
            false,
        ));

        let program_id = get_last_program_id();
        run_to_next_block(None);

        let duration_a: BlockNumber = 5;
        let duration_b: BlockNumber = 10;
        let payload = Command::WaitLost(USER_1.cast()).encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            30_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        assert!(MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| {
            if msg.payload_bytes() == b"ping" {
                assert_ok!(Gear::send_reply(
                    RuntimeOrigin::signed(USER_1),
                    msg.id(),
                    b"ping".to_vec(),
                    100_000_000,
                    0,
                    false,
                ));

                true
            } else {
                false
            }
        }));

        let now = System::block_number();
        let targets = [duration_a, duration_b].map(|target| target + now - 1);
        let run_to_target = |target: BlockNumber| {
            System::set_block_number(target);
            Gear::set_block_number(target);
            run_to_next_block(None);
        };

        // Run to the end of the first duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[0]);
        assert!(
            !MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes() == b"unreachable")
        );

        // Run to the end of the second duration.
        //
        // The timeout message has been triggered.
        run_to_target(targets[1]);
        assert!(
            MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload_bytes() == b"timeout")
        );
        assert!(
            MailboxOf::<Test>::iter_key(USER_1)
                .any(|(msg, _bn)| msg.payload_bytes() == b"timeout2")
        );
        assert!(
            MailboxOf::<Test>::iter_key(USER_1).any(|(msg, _bn)| msg.payload_bytes() == b"success")
        );
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
            1_000,
            false,
        ));

        let skipped_message_id = get_last_message_id();
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        run_to_block(2, None);

        assert_not_executed(skipped_message_id);

        // some funds may be unreserved after processing init-message
        assert!(user_balance_before <= Balances::free_balance(USER_1));

        assert!(!utils::is_active(program_id));
        assert!(<Test as Config>::CodeStorage::original_code_exists(
            code_hash
        ));
    })
}

#[test]
fn exit_locking_funds() {
    use demo_constructor::{Calls, Scheme};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, program_id) = init_constructor(Scheme::empty());
        let ed = get_ed();

        let user_2_balance = Balances::free_balance(USER_2);

        assert!(Gear::is_initialized(program_id));

        assert_program_balance(program_id, 0_u128, ed, 0_u128);

        let value = 1_000;

        let calls = Calls::builder().send_value(program_id.into_bytes(), [], value);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            calls.encode(),
            10_000_000_000,
            value,
            false,
        ));
        let message_1 = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(message_1);

        let calls = Calls::builder().exit(<[u8; 32]>::from(USER_2.into_origin()));
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            calls.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let message_2 = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(message_2);

        // Both `value` and ED from the program's account go to the USER_2 as beneficiary.
        assert_balance(USER_2, user_2_balance + value + ed, 0u128);
        assert_balance(program_id, 0u128, 0u128);

        // nothing should change
        assert_ok!(Gear::claim_value_to_inheritor(
            RuntimeOrigin::signed(USER_1),
            program_id,
            NonZero::<u32>::MAX,
        ));

        run_to_next_block(None);

        assert_balance(USER_2, user_2_balance + value + ed, 0u128);
        assert_balance(program_id, 0u128, 0u128);
    });
}

#[test]
fn frozen_funds_remain_on_exit() {
    use crate::{Fortitude, Preservation, fungible};
    use demo_constructor::{Calls, Scheme};
    use frame_support::traits::{LockableCurrency, WithdrawReasons};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, program_id) = init_constructor(Scheme::empty());
        let ed = get_ed();

        let program_account = program_id.cast();

        // This doesn't include the ED that has already been locked on the `program_account`.
        let user_2_initial_balance = CurrencyOf::<Test>::free_balance(USER_2);

        assert!(Gear::is_initialized(program_id));

        // Funding program account with this amount
        let value = 5_000;

        let calls = Calls::builder().send_value(program_id.into_bytes(), [], value);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            calls.encode(),
            10_000_000_000,
            value,
            false,
        ));

        run_to_next_block(None);

        // The ED is not offset against `value` but locked separately.
        assert_program_balance(program_id, value, ed, 0_u128);

        // reducible balance of an active program doesn't include the ED (runtime guarantee)
        assert_eq!(
            <CurrencyOf<Test> as fungible::Inspect<_>>::reducible_balance(
                &program_account,
                Preservation::Expendable,
                Fortitude::Polite,
            ),
            value
        );

        <CurrencyOf<Test> as LockableCurrency<AccountId>>::set_lock(
            *b"py/grlok",
            &program_account,
            value / 2,
            WithdrawReasons::all(),
        );

        // `exit()` will trigger program deactivation
        let calls = Calls::builder().exit(<[u8; 32]>::from(USER_2.into_origin()));
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            calls.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let message_id = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(message_id);

        // Frozen amount was not allowed to have been transferred to the beneficiary.
        assert_eq!(CurrencyOf::<Test>::free_balance(program_account), value / 2);
        // The beneficiary's account has only been topped up with half of the program's balance.
        // In addition to that the ED has been transferred to the beneficiary, as well.
        assert_eq!(
            CurrencyOf::<Test>::free_balance(USER_2),
            user_2_initial_balance + value / 2 + ed
        );
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
        let code = <Test as Config>::CodeStorage::get_instrumented_code(code_id)
            .expect("code should be in the storage");
        let code_length = code.bytes().len() as u64;
        let system_reservation = demo_init_fail_sender::system_reserve();
        let reply_duration = demo_init_fail_sender::reply_duration();

        let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
        let gas_for_module_instantiation = {
            let instantiated_section_sizes = code.instantiated_section_sizes();

            let instantiation_weights = schedule.instantiation_weights;

            let mut gas_for_code_instantiation = instantiation_weights
                .code_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.code_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .data_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.data_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .global_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.global_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .table_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.table_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .element_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.element_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .type_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.type_section() as u64);

            gas_for_code_instantiation
        };
        let gas_for_code_len = read_cost;
        let gas_for_program = read_cost;
        let gas_for_code = schedule
            .db_weights
            .read_per_byte
            .ref_time()
            .saturating_mul(code_length)
            .saturating_add(read_cost);

        // Additional gas for loading resources on next wake up.
        // Must be exactly equal to gas, which we must pre-charge for program execution.
        let gas_for_second_init_execution =
            gas_for_program + gas_for_code_len + gas_for_code + gas_for_module_instantiation;

        let ed = get_ed();

        // Value which must be returned to `USER1` after init message processing complete.
        let prog_free = 4000u128;
        // Reserved value, which is sent to user in init and then we wait for reply from user.
        let prog_reserve = 1000u128;

        let locked_gas_to_wl = CostsPerBlockOf::<Test>::waitlist()
            * GasBalanceOf::<Test>::saturated_from(
                reply_duration.saturated_into::<u64>() + CostsPerBlockOf::<Test>::reserve_for(),
            );
        let gas_spent_in_wl = CostsPerBlockOf::<Test>::waitlist();
        // Value, which will be returned to init message after wake.
        let returned_from_wait_list = gas_price(locked_gas_to_wl - gas_spent_in_wl);

        // Value, which will be returned to `USER1` after init message processing complete.
        let returned_from_system_reservation = gas_price(system_reservation);

        // Since we set the gas for the second execution of the init message only for resource loading,
        // after execution, the system-reserved gas, the sent value, and the price for the waitlist must
        // be returned to the user. This is because the program will stop its execution on the first wasm
        // block due to exceeding the gas limit. Therefore, the gas counter will equal the amount of gas
        // returned from the waitlist in the handle reply.
        let expected_balance_difference =
            prog_free + returned_from_wait_list + returned_from_system_reservation + ed;

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            DEFAULT_SALT.to_vec(),
            USER_3.into_origin().encode(),
            gas_spent_init + gas_for_second_init_execution,
            5_000u128,
            false
        ));

        let program_id = get_last_program_id();
        let message_id = get_last_message_id();

        // Before the `init` message is processed the program only has ED on its account.
        assert_program_balance(program_id, 0_u128, ed, 0_u128);

        run_to_next_block(None);

        assert!(utils::is_active(program_id));
        assert_program_balance(program_id, prog_free, ed, prog_reserve);

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
            HandleKind::Reply(
                message_to_reply,
                ReplyCode::Success(SuccessReplyReason::Manual),
            ),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_to_reply,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent_reply,
            0,
            false,
        ));

        let reply_id = get_last_message_id();

        let user_1_balance = Balances::free_balance(USER_1);
        let user_3_balance = Balances::free_balance(USER_3);

        run_to_next_block(None);

        assert_succeed(reply_id);
        assert_failed(
            message_id,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );
        assert!(Gear::is_terminated(program_id));
        // ED has been returned to the beneficiary as a part of the `free` balance.
        // The `reserved` part of the balance is still being held.
        assert_program_balance(program_id, 0_u128, 0_u128, prog_reserve);

        let expected_balance = user_1_balance + expected_balance_difference;
        let user_1_balance = Balances::free_balance(USER_1);

        assert_eq!(user_1_balance, expected_balance);

        // Hack to fast spend blocks till expiration.
        System::set_block_number(interval.finish - 1);
        Gear::set_block_number(interval.finish - 1);

        run_to_next_block(None);

        assert!(MailboxOf::<Test>::is_empty(&USER_3));

        let extra_gas_to_mb = gas_price(
            CostsPerBlockOf::<Test>::mailbox()
                * GasBalanceOf::<Test>::saturated_from(CostsPerBlockOf::<Test>::reserve_for()),
        );

        assert_program_balance(program_id, 0u128, 0u128, 0u128);
        assert_eq!(
            Balances::free_balance(USER_3),
            user_3_balance + prog_reserve
        );
        assert_eq!(
            Balances::free_balance(USER_1),
            user_1_balance + extra_gas_to_mb
        );

        // nothing should change
        assert_ok!(Gear::claim_value_to_inheritor(
            RuntimeOrigin::signed(USER_1),
            program_id,
            NonZero::<u32>::MAX,
        ));

        run_to_next_block(None);

        assert_balance(program_id, 0u128, 0u128);
        assert_eq!(
            Balances::free_balance(USER_3),
            user_3_balance + prog_reserve
        );
        assert_eq!(
            Balances::free_balance(USER_1),
            user_1_balance + extra_gas_to_mb,
        );
    });
}

#[test]
fn claim_value_to_inheritor() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, pid1) =
            submit_constructor_with_args(USER_1, "constructor1", Scheme::empty(), 0);
        let (_init_mid, pid2) =
            submit_constructor_with_args(USER_1, "constructor2", Scheme::empty(), 0);
        let (_init_mid, pid3) =
            submit_constructor_with_args(USER_1, "constructor3", Scheme::empty(), 0);

        run_to_next_block(None);

        // also, noop cases are in `*_locking_funds` tests
        assert_noop!(
            Gear::claim_value_to_inheritor(
                RuntimeOrigin::signed(USER_1),
                pid1,
                NonZero::<u32>::MAX
            ),
            Error::<Test>::ActiveProgram
        );

        run_to_next_block(None);

        let user_2_balance = Balances::free_balance(USER_2);

        // add value to `pid1`
        let value1 = 1_000;
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid1,
            Calls::default().encode(),
            10_000_000_000,
            value1,
            false,
        ));
        let value_mid1 = utils::get_last_message_id();

        // add value to `pid2`
        let value2 = 4_000;
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            Calls::default().encode(),
            10_000_000_000,
            value2,
            false,
        ));
        let value_mid2 = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(value_mid1);
        assert_succeed(value_mid2);

        let ed = get_ed();
        assert_program_balance(pid1, value1, ed, 0u128);
        assert_program_balance(pid2, value2, ed, 0u128);
        assert_program_balance(pid3, 0u128, ed, 0u128);

        // exit in reverse order so the chain of inheritors will not transfer
        // all the balances to `USER_2`

        // make `pid3` exit and refer to `USER_2`
        let calls = Calls::builder().exit(<[u8; 32]>::from(USER_2.into_origin()));
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid3,
            calls.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let mid3 = utils::get_last_message_id();

        // make `pid2` exit and refer to `pid3`
        let calls = Calls::builder().exit(pid3.into_bytes());
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid2,
            calls.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let mid2 = utils::get_last_message_id();

        // make `pid1` exit and refer to `pid2`
        let calls = Calls::builder().exit(pid2.into_bytes());
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid1,
            calls.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let mid1 = utils::get_last_message_id();

        run_to_next_block(None);

        assert_succeed(mid1);
        assert_succeed(mid2);
        assert_succeed(mid3);

        // `pid1` transferred all the balances to `pid2`
        assert_program_balance(pid1, 0u128, 0u128, 0u128);
        // `pid2` transferred `value2` and `ed` to `pid3`
        // but have `value1` and `ed` of `pid1`
        assert_balance(pid2, value1 + ed, 0u128);
        // `pid3` transferred its 0 value and `ed` to `USER_2`
        // but have `value2` and `ed` of `pid2`
        assert_balance(pid3, value2 + ed, 0u128);
        // `USER_2` have `ed` of `pid3`
        assert_balance(USER_2, user_2_balance + ed, 0u128);

        assert_ok!(Gear::claim_value_to_inheritor(
            RuntimeOrigin::signed(USER_1),
            pid1,
            NonZero::<u32>::MAX,
        ));

        run_to_next_block(None);

        assert_program_balance(pid1, 0u128, 0u128, 0u128);
        assert_program_balance(pid2, 0u128, 0u128, 0u128);
        assert_program_balance(pid3, 0u128, 0u128, 0u128);
        assert_balance(USER_2, user_2_balance + value1 + value2 + 3 * ed, 0u128);
    });
}

#[test]
fn test_sequence_inheritor_of() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (builtins, _) = <Test as Config>::BuiltinDispatcherFactory::create();
        let manager = ExtManager::<Test>::new(builtins);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            demo_ping::WASM_BINARY.to_vec(),
        ));
        let code_id = get_last_code_id();

        let message_id = MessageId::from(1);

        // serial inheritance
        let mut programs = vec![];
        for i in 1000..1100 {
            let program_id = i.cast();
            manager.set_program(program_id, code_id, message_id, 1.unique_saturated_into());

            ProgramStorageOf::<Test>::update_program_if_active(program_id, |program, _bn| {
                let inheritor = programs.last().copied().unwrap_or_else(|| USER_1.cast());
                if i.is_multiple_of(2) {
                    *program = Program::Exited(inheritor);
                } else {
                    *program = Program::Terminated(inheritor);
                }
            })
            .unwrap();

            programs.push(program_id);
        }

        let indexed_programs: Vec<u64> = (1000..1100).collect();
        let convert_holders = |holders: BTreeSet<ActorId>| {
            let mut holders = holders
                .into_iter()
                .map(|x| u64::from_le_bytes(*x.into_bytes().split_first_chunk::<8>().unwrap().0))
                .collect::<Vec<u64>>();
            holders.sort();
            holders
        };
        let inheritor_for = |id, max_depth| {
            let max_depth = NonZero::<usize>::new(max_depth).unwrap();
            let (inheritor, holders) = Gear::inheritor_for(id, max_depth).unwrap();
            let holders = convert_holders(holders);
            (inheritor, holders)
        };

        let res = Gear::inheritor_for(USER_1.cast(), NonZero::<usize>::MAX);
        assert_eq!(res, Err(InheritorForError::NotFound));

        let (inheritor, holders) = inheritor_for(programs[99], usize::MAX);
        assert_eq!(inheritor, USER_1.cast());
        assert_eq!(holders, indexed_programs);

        let (inheritor, holders) = inheritor_for(programs[49], usize::MAX);
        assert_eq!(inheritor, USER_1.cast());
        assert_eq!(holders, indexed_programs[..=49]);

        let (inheritor, holders) = inheritor_for(programs[0], usize::MAX);
        assert_eq!(inheritor, USER_1.cast());
        assert_eq!(holders, indexed_programs[..=0]);

        let (inheritor, holders) = inheritor_for(programs[0], 1);
        assert_eq!(inheritor, USER_1.cast());
        assert_eq!(holders, indexed_programs[..=0]);

        let (inheritor, holders) = inheritor_for(programs[99], 10);
        assert_eq!(inheritor, programs[89]);
        assert_eq!(holders, indexed_programs[90..]);

        let (inheritor, holders) = inheritor_for(programs[99], 50);
        assert_eq!(inheritor, programs[49]);
        assert_eq!(holders, indexed_programs[50..]);

        let (inheritor, holders) = inheritor_for(programs[99], 100);
        assert_eq!(inheritor, USER_1.cast());
        assert_eq!(holders, indexed_programs);

        let (inheritor, holders) = inheritor_for(programs[99], 99);
        assert_eq!(inheritor, programs[0]);
        assert_eq!(holders, indexed_programs[1..]);
    });
}

#[test]
fn test_cyclic_inheritor_of() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (builtins, _) = <Test as Config>::BuiltinDispatcherFactory::create();
        let manager = ExtManager::<Test>::new(builtins);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            demo_ping::WASM_BINARY.to_vec(),
        ));
        let code_id = get_last_code_id();

        let message_id = MessageId::from(1);

        // cyclic inheritance
        let mut cyclic_programs = vec![];
        for i in 2000..2100 {
            let program_id = ActorId::from(i);
            manager.set_program(program_id, code_id, message_id, 1.unique_saturated_into());

            ProgramStorageOf::<Test>::update_program_if_active(program_id, |program, _bn| {
                let inheritor = cyclic_programs
                    .last()
                    .copied()
                    .unwrap_or_else(|| 2099.into());
                if i.is_multiple_of(2) {
                    *program = Program::Exited(inheritor);
                } else {
                    *program = Program::Terminated(inheritor);
                }
            })
            .unwrap();

            cyclic_programs.push(program_id);
        }

        let cyclic_programs: BTreeSet<ActorId> = cyclic_programs.into_iter().collect();

        let res = Gear::inheritor_for(2000.into(), NonZero::<usize>::MAX);
        assert_eq!(
            res,
            Err(InheritorForError::Cyclic {
                holders: cyclic_programs.clone(),
            })
        );

        let res = Gear::inheritor_for(2000.into(), NonZero::<usize>::new(101).unwrap());
        assert_eq!(
            res,
            Err(InheritorForError::Cyclic {
                holders: cyclic_programs
            })
        );

        let (inheritor, _holders) =
            Gear::inheritor_for(2000.into(), NonZero::<usize>::new(100).unwrap()).unwrap();
        assert_eq!(inheritor, 2000.into());

        let (inheritor, _holders) =
            Gear::inheritor_for(2000.into(), NonZero::<usize>::new(99).unwrap()).unwrap();
        assert_eq!(inheritor, 2001.into());
    });
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
            schedule.limits.data_segments_amount.into(),
        )
        .expect("Code failed to load");

        let code_id = CodeId::generate(code.original_code());
        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            vec![],
            Vec::new(),
            // # TODO
            //
            // Calculate the gas spent after #1242.
            10_000_000_000u64,
            0u128,
            false,
        ));

        let program_id = utils::get_last_program_id();

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

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
            false,
        ));

        run_to_next_block(None);

        assert!(Gear::is_initialized(program_id));
    })
}

#[test]
fn test_create_program_no_code_hash() {
    use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};

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
            4 * get_ed(),
            false,
        ));

        // Try to create a program with non existing code hash
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            50_000_000_000,
            0,
            false,
        ));
        run_to_block(2, None);

        // Init and dispatch messages from the program are dequeued, but not executed
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
            false,
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
            false,
        ));

        run_to_block(4, None);

        assert_eq!(MailboxOf::<Test>::len(&USER_2), 6);
        assert_total_dequeued(12 + 1);
        assert_init_success(0);
    });
}

#[test]
fn test_create_program_simple() {
    use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};

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
            6 * get_ed(),
            false,
        ));
        run_to_block(2, None);

        // Test create one successful in init program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Default.encode(),
            50_000_000_000,
            0,
            false,
        ));
        run_to_block(3, None);

        // Test create one failing in init program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(
                vec![(child_code_hash, b"some_data".to_vec(), 150_000)] // too little gas
            )
            .encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_block(4, None);

        // First extrinsic call with successful program creation dequeues and executes init and dispatch messages
        // Second extrinsic is failing one, for each message it generates replies, which are executed (4 dequeued, 2 dispatched)
        assert_total_dequeued(6 + 3 + 2); // +3 for extrinsics +2 for auto generated replies
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
            false,
        ));
        run_to_block(5, None);

        // Create multiple successful init programs
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                (child_code_hash, b"salt3".to_vec(), 150_000), // too little gas
                (child_code_hash, b"salt4".to_vec(), 150_000), // too little gas
            ])
            .encode(),
            50_000_000_000,
            0,
            false,
        ));
        run_to_block(6, None);

        assert_total_dequeued(12 + 2 + 4); // +2 for extrinsics +4 for auto generated replies
        assert_init_success(2);
    })
}

#[test]
fn state_request() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_custom::{
            InitMessage, WASM_BINARY,
            btree::{Request, StateRequest},
        };

        let code = WASM_BINARY;
        let program_id = generate_program_id(code, DEFAULT_SALT);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_2),
            code.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitMessage::BTree.encode(),
            50_000_000_000,
            0,
            false,
        ));

        let data = [(0u32, 1u32), (2, 4), (7, 8)];
        for (key, value) in data {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                program_id,
                Request::Insert(key, value).encode(),
                1_000_000_000,
                0,
                false,
            ));
        }

        run_to_next_block(None);

        for (key, value) in data {
            let ret = Gear::read_state_impl(program_id, StateRequest::ForKey(key).encode(), None)
                .unwrap();
            assert_eq!(
                Option::<u32>::decode(&mut ret.as_slice()).unwrap().unwrap(),
                value
            );
        }

        let ret = Gear::read_state_impl(program_id, StateRequest::Full.encode(), None).unwrap();
        let ret = BTreeMap::<u32, u32>::decode(&mut ret.as_slice()).unwrap();
        let expected: BTreeMap<u32, u32> = data.into_iter().collect();
        assert_eq!(ret, expected);
    })
}

#[test]
fn test_create_program_duplicate() {
    use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};
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
            4 * get_ed(),
            false,
        ));
        run_to_block(2, None);

        // User creates a program
        assert_ok!(upload_program_default(USER_1, ProgramCodeKind::Default));
        run_to_block(3, None);

        // Program creates identical program
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
            false,
        ));
        run_to_block(4, None);

        assert_total_dequeued(3 + 3 + 1); // +3 from extrinsics (2 upload_program, 1 send_message) +1 for auto generated reply
        assert_init_success(3); // (3 upload_program)

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
            false,
        ));
        run_to_block(5, None);

        // Create an identical program from program
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![(child_code_hash, b"salt1".to_vec(), 2_000_000_000)])
                .encode(),
            20_000_000_000,
            0,
            false,
        ));
        run_to_block(6, None);

        assert_total_dequeued(5 + 2 + 3); // +2 from extrinsics (send_message) +3 for auto generated replies
        assert_init_success(2); // Both uploads succeed due to unique program id generation

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            child_code,
            b"salt1".to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));
    });
}

#[test]
fn test_create_program_duplicate_in_one_execution() {
    use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};

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
            2 * get_ed(),
            false,
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
            false,
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
            false,
        ));

        run_to_block(4, None);

        assert_total_dequeued(2 + 1 + 2); // 1 for extrinsics +2 for auto generated replies
        assert_init_success(1);
    });
}

#[test]
fn test_create_program_miscellaneous() {
    use demo_program_factory::{CreateProgram, WASM_BINARY as PROGRAM_FACTORY_WASM_BINARY};

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
            4 * get_ed(),
            false,
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
            false,
        ));

        run_to_block(3, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            factory_id,
            CreateProgram::Custom(vec![
                // init fail (not enough gas) and reply generated (+2 dequeued, +1 dispatched),
                // handle message is processed, but not executed, reply generated (+2 dequeued, +1 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 150_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt2".to_vec(), 200_000_000),
            ])
            .encode(),
            50_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            factory_id,
            CreateProgram::Custom(vec![
                // duplicate in the next block: init is executed due to new ActorId generation, replies are generated (+4 dequeue, +2 dispatched)
                (child2_code_hash, b"salt1".to_vec(), 200_000_000),
                // one successful init with one handle message (+2 dequeued, +1 dispatched, +1 successful init)
                (child2_code_hash, b"salt3".to_vec(), 200_000_000),
            ])
            .encode(),
            50_000_000_000,
            0,
            false,
        ));

        run_to_block(5, None);

        assert_total_dequeued(18 + 4 + 6); // +4 for 3 send_message calls and 1 upload_program call +6 for auto generated replies
        assert_init_success(4 + 1); // +1 for submitting factory
    });
}

#[test]
fn exit_handle() {
    use demo_constructor::{WASM_BINARY, demo_exit_handle};

    init_logger();
    new_test_ext().execute_with(|| {
        let code_id = CodeId::generate(WASM_BINARY);

        let (_init_mid, program_id) = init_constructor(demo_exit_handle::scheme());

        // An expensive operation since "gr_exit" removes all program pages from storage.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            50_000_000_000u64,
            0u128,
            false,
        ));

        run_to_block(3, None);

        assert!(!utils::is_active(program_id));
        assert!(MailboxOf::<Test>::is_empty(&USER_3));
        assert!(!Gear::is_initialized(program_id));
        assert!(!utils::is_active(program_id));

        assert!(<Test as Config>::CodeStorage::original_code_exists(code_id));

        // Program is not removed and can't be submitted again
        assert_noop!(
            Gear::create_program(
                RuntimeOrigin::signed(USER_1),
                code_id,
                DEFAULT_SALT.to_vec(),
                Vec::new(),
                2_000_000_000,
                0u128,
                false,
            ),
            Error::<Test>::ProgramAlreadyExists,
        );
    })
}

#[test]
fn no_redundant_gas_value_after_exiting() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_constructor::demo_exit_handle;

        let ed = get_ed();

        let (_init_mid, prog_id) = init_constructor(demo_exit_handle::scheme());

        let GasInfo {
            min_limit: gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog_id,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent,
            0,
            false,
        ));

        let msg_id = get_last_message_id();
        assert_ok!(GasHandlerOf::<Test>::get_limit(msg_id), gas_spent);

        // before execution
        let free_after_send = Balances::free_balance(USER_1);
        let reserved_after_send = GearBank::<Test>::account_total(&USER_1);
        assert_eq!(reserved_after_send, gas_price(gas_spent));

        run_to_block(3, None);

        // gas_limit has been recovered
        assert_noop!(
            GasHandlerOf::<Test>::get_limit(msg_id),
            pallet_gear_gas::Error::<Test>::NodeNotFound
        );

        // the (reserved_after_send - gas_spent) has been unreserved, the `ed` has been returned
        let free_after_execution = Balances::free_balance(USER_1);
        assert_eq!(
            free_after_execution,
            free_after_send + reserved_after_send - gas_price(gas_spent) + ed
        );

        // reserved balance after execution is zero
        let reserved_after_execution = GearBank::<Test>::account_total(&USER_1);
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
            0u128,
            false,
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
                0u128,
                false,
            ));
        }

        // block 3
        //
        // - assert responses in events
        // - reply and wake program
        // - check program status
        run_to_block(3, None);

        let mut responses = vec![
            Assertion::ReplyCode(ReplyCode::Error(
                ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::Uninitialized)
            ));
            count
        ];
        responses.insert(0, Assertion::Payload(vec![])); // init response
        assert_responses_to_user(USER_1, responses);

        assert_eq!(WaitlistOf::<Test>::iter_key(pid).count(), 1);

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
            false,
        ));

        assert!(!Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        // block 4
        //
        // - check if program has terminated
        // - check wait list is empty
        run_to_block(4, None);
        assert!(!Gear::is_initialized(pid));
        assert!(!utils::is_active(pid));
        assert_eq!(WaitlistOf::<Test>::iter_key(pid).count(), 0);
    })
}

#[test]
fn locking_gas_for_waitlist() {
    use demo_constructor::{Calls, Scheme};
    use demo_gas_burned::WASM_BINARY as GAS_BURNED_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let waiter = upload_program_default(USER_1, ProgramCodeKind::Custom(utils::WAITER_WAT))
            .expect("submit result was asserted");

        // This program just does some calculations (burns gas) on each handle message.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            GAS_BURNED_BINARY.to_vec(),
            Default::default(),
            Default::default(),
            100_000_000_000,
            0,
            false,
        ));
        let calculator = get_last_program_id();

        // This program sends two empty gasless messages on each handle:
        // for this test first message is waiter, seconds is calculator.
        let (_init_mid, sender) =
            submit_constructor_with_args(USER_1, DEFAULT_SALT, Scheme::empty(), 0);

        run_to_block(2, None);

        assert!(Gear::is_initialized(waiter));
        assert!(Gear::is_initialized(calculator));
        assert!(Gear::is_initialized(sender));

        let calls = Calls::builder()
            .send(calculator.into_bytes(), [])
            .send(waiter.into_bytes(), []);

        calculate_handle_and_send_with_extra(USER_1, sender, calls.encode(), None, 0);
        let origin_msg_id = get_last_message_id();

        let message_to_be_waited = MessageId::generate_outgoing(origin_msg_id, 1);

        run_to_block(3, None);

        assert!(WaitlistOf::<Test>::contains(&waiter, &message_to_be_waited));

        let expiration = utils::get_waitlist_expiration(message_to_be_waited);

        // Expiration block may be really far from current one, so proper
        // `run_to_block` takes a lot, so we use hack here by setting
        // close block number to it to check that messages keeps in
        // waitlist before and leaves it as expected.
        System::set_block_number(expiration - 2);
        Gear::set_block_number(expiration - 2);

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
            true,
        )
        .unwrap();

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            code_id,
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            gas_info.min_limit,
            0,
            false,
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
    use demo_custom::{InitMessage, WASM_BINARY, btree::Request};

    init_logger();
    new_test_ext().execute_with(|| {
        let initial_balance = Balances::free_balance(USER_1);
        let ed = get_ed();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitMessage::BTree.encode(),
            50_000_000_000,
            0,
            false,
        ));

        let prog_id = utils::get_last_program_id();

        run_to_block(2, None);

        let balance_after_init = Balances::free_balance(USER_1);

        let request = Request::Clear.encode();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog_id,
            request.clone(),
            10_000_000_000,
            0,
            false,
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
            InitMessage::BTree.encode(),
            0,
            true,
            true,
        )
        .unwrap();

        // check that all changes made by calculate_gas_info are rollbacked
        assert_eq!(balance_after_handle, Balances::free_balance(USER_1));
        assert_eq!(total_balance_after_handle, Balances::total_balance(&USER_1));

        assert_eq!(
            (initial_balance - balance_after_init),
            gas_price(init_gas_spent) + ed
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
            true,
        )
        .unwrap();

        assert_eq!(
            balance_after_init - balance_after_handle,
            gas_price(handle_gas_spent)
        );
    });
}

#[test]
fn gas_spent_precalculated() {
    // After instrumentation will be:
    // (export "handle" (func $handle_export))
    // (func $add
    //      <-- call gas_charge -->
    //      local.get $0
    //      local.get $1
    //      i32.add
    //      local.set $2
    // )
    // (func $handle
    //      <-- call gas_charge -->
    //      <-- stack limit check and increase -->
    //      call $add (i32.const 2) (i32.const 2))
    //      <-- stack limit decrease -->
    // )
    // (func $handle_export
    //      <-- call gas_charge -->
    //      <-- stack limit check and increase -->
    //      call $handle
    //      <-- stack limit decrease -->
    // )
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

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = upload_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("submit result was asserted");

        run_to_block(2, None);

        let get_program_code = |pid| {
            let code_id = ProgramStorageOf::<Test>::get_program(pid)
                .and_then(|program| ActiveProgram::try_from(program).ok())
                .expect("program must exist")
                .code_id
                .cast();

            <Test as Config>::CodeStorage::get_instrumented_code(code_id).unwrap()
        };

        let get_gas_charged_for_code = |pid| {
            let schedule = <Test as Config>::Schedule::get();
            let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
            let instrumented_prog = get_program_code(pid);
            let code_len = instrumented_prog.bytes().len() as u64;
            let gas_for_code_read = schedule
                .db_weights
                .read_per_byte
                .ref_time()
                .saturating_mul(code_len)
                .saturating_add(read_cost);

            let instantiated_section_sizes = instrumented_prog.instantiated_section_sizes();

            let instantiation_weights = schedule.instantiation_weights;

            let mut gas_for_code_instantiation = instantiation_weights
                .code_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.code_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .data_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.data_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .global_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.global_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .table_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.table_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .element_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.element_section() as u64);

            gas_for_code_instantiation += instantiation_weights
                .type_section_per_byte
                .ref_time()
                .saturating_mul(instantiated_section_sizes.type_section() as u64);

            gas_for_code_read + gas_for_code_instantiation
        };

        let instrumented_code = get_program_code(pid);
        let module = Module::new(instrumented_code.bytes()).expect("invalid wasm bytes");

        let (handle_export_func_body, gas_charge_func_body) = module
            .code_section
            .as_ref()
            .and_then(|section| match &section[..] {
                [.., handle_export, gas_charge] => Some((handle_export, gas_charge)),
                _ => None,
            })
            .expect("failed to locate `handle_export()` and `gas_charge()` functions");

        let gas_charge_call_cost = gas_charge_func_body
            .instructions
            .iter()
            .find_map(|instruction| match instruction {
                Instruction::I64Const(value) => Some(*value as u64),
                _ => None,
            })
            .expect("failed to get cost of `gas_charge()` function");

        let handle_export_instructions = &handle_export_func_body.instructions;
        assert!(matches!(
            handle_export_instructions[..],
            [
                Instruction::I32Const { .. }, //stack check limit cost
                Instruction::Call { .. },     //call to `gas_charge()`
                ..
            ]
        ));

        macro_rules! cost {
            ($name:ident) => {
                <Test as Config>::Schedule::get().instruction_weights.$name as u64
            };
        }

        let stack_check_limit_cost = handle_export_instructions
            .iter()
            .find_map(|instruction| match instruction {
                Instruction::I32Const(value) => Some(*value as u64),
                _ => None,
            })
            .expect("failed to get stack check limit cost")
            - cost!(call);

        let gas_spent_expected = {
            let execution_cost = cost!(call) * 2
                + cost!(i64const) * 2
                + cost!(local_set)
                + cost!(local_get) * 2
                + cost!(i32add)
                + gas_charge_call_cost * 3
                + stack_check_limit_cost * 2;

            let read_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
            execution_cost
                // cost for loading program
                + read_cost
                // cost for loading code length
                + read_cost
                // cost for code loading and instantiation
                + get_gas_charged_for_code(pid)
        };

        let make_check = |gas_spent_expected| {
            let GasInfo {
                min_limit: gas_spent_calculated,
                ..
            } = Gear::calculate_gas_info(
                USER_1.into_origin(),
                HandleKind::Handle(pid),
                EMPTY_PAYLOAD.to_vec(),
                0,
                true,
                true,
            )
            .unwrap();

            assert_eq!(gas_spent_calculated, gas_spent_expected);
        };

        // Check also, that gas spent is the same if we calculate it twice.
        make_check(gas_spent_expected);
        make_check(gas_spent_expected);
    });
}

#[test]
fn test_two_programs_composition_works() {
    use demo_compose::WASM_BINARY as COMPOSE_WASM_BINARY;
    use demo_mul_by_const::WASM_BINARY as MUL_CONST_WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_a_id = generate_program_id(MUL_CONST_WASM_BINARY, b"program_a");
        let program_b_id = generate_program_id(MUL_CONST_WASM_BINARY, b"program_b");
        let program_code_id = CodeId::generate(MUL_CONST_WASM_BINARY);
        let compose_id = generate_program_id(COMPOSE_WASM_BINARY, b"salt");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"program_a".to_vec(),
            50_u64.encode(),
            10_000_000_000,
            0,
            false,
        ));

        assert_ok!(Gear::create_program(
            RuntimeOrigin::signed(USER_1),
            program_code_id,
            b"program_b".to_vec(),
            75_u64.encode(),
            10_000_000_000,
            0,
            false,
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            COMPOSE_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            (
                <[u8; 32]>::from(program_a_id),
                <[u8; 32]>::from(program_b_id)
            )
                .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            compose_id,
            100_u64.to_le_bytes().to_vec(),
            60_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        // Gas total issuance should have gone back to 0.
        assert_eq!(utils::user_messages_sent(), (4, 0));
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    });
}

// Passing value less than the ED to newly-created programs is now legal since the account for a
// program-in-creation is guaranteed to exists before the program gets stored in `ProgramStorage`.
// Both `uploade_program` (`create_program`) extrinsic and the `create_program` syscall should
// successfully handle such cases.
#[test]
fn test_create_program_with_value_lt_ed() {
    use demo_constructor::{Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Ids of custom_destination
        let ed = get_ed();
        let msg_receiver_1 = 5u64;
        let msg_receiver_1_hash = <[u8; 32]>::from(msg_receiver_1.into_origin());
        let msg_receiver_2 = 6u64;
        let msg_receiver_2_hash = <[u8; 32]>::from(msg_receiver_2.into_origin());

        let default_calls = Calls::builder()
            .send_value(msg_receiver_1_hash, [], 500)
            .send_value(msg_receiver_2_hash, [], 500);

        // Submit the code
        let code = ProgramCodeKind::Default.to_bytes();
        let code_id = CodeId::generate(&code).into_bytes();
        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), code));

        // Initialization of a program with value less than ED is allowed
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
            b"test0".to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            ed - 1,
            false,
        ));

        let gas_limit = 200_000_001;

        // Simple passing test with values.
        // Sending 2 x 500 value with "handle" messages. This should not fail.
        // Should be noted, that "handle" messages send value to some non-existing address
        // therefore messages will go to the mailbox.
        let calls = default_calls
            .clone()
            .create_program_wgas(code_id, [], [], gas_limit);

        let (_init_mid, _pid) =
            submit_constructor_with_args(USER_1, b"test1", Scheme::direct(calls), 1_500);

        run_to_block(2, None);

        // 3 init messages and 1 reply
        assert_total_dequeued(3 + 1);
        // 2 programs deployed by the user and 1 program created by a program
        assert_init_success(3);

        let origin_msg_id = MessageId::generate_from_user(1, USER_1.cast(), 1);
        let msg1_mailbox = MessageId::generate_outgoing(origin_msg_id, 0);
        let msg2_mailbox = MessageId::generate_outgoing(origin_msg_id, 1);
        assert!(MailboxOf::<Test>::contains(&msg_receiver_1, &msg1_mailbox));
        assert!(MailboxOf::<Test>::contains(&msg_receiver_2, &msg2_mailbox));

        System::reset_events();

        // Trying to send init message from program with value less than ED.
        // All messages popped from the queue should succeed regardless of the value in transfer.
        let calls = default_calls.create_program_value_wgas(code_id, [], [], gas_limit, ed - 1);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"test2".to_vec(),
            Scheme::direct(calls).encode(),
            10_000_000_000,
            10_000,
            false,
        ));

        run_to_block(3, None);

        assert_total_dequeued(3);
    })
}

// Before introducing this test, upload_program extrinsic didn't check the value.
// Also value wasn't check in `create_program` syscall. There could be the next test case, which could affect badly.
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
    use demo_constructor::{Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let msg_value = 100001;
        let calls = Calls::builder().create_program_value([0; 32], [], [], msg_value);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Scheme::direct(calls).encode(),
            10_000_000_000,
            msg_value - 1,
            false,
        ));

        let msg_id = get_last_message_id();

        run_to_next_block(None);

        // User's message execution will result in trap, because program tries
        // to send init message with value in invalid range.
        assert_total_dequeued(1);

        let error_text = format!(
            "panicked with 'Failed to create program: {:?}'",
            CoreError::Ext(ExtError::Execution(ExecutionError::NotEnoughValue))
        );
        assert_failed(msg_id, AssertFailedError::Panic(error_text));
    })
}

#[test]
fn test_create_program_without_gas_works() {
    use demo_constructor::{Calls, Scheme};

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Default.to_bytes();
        let code_id = CodeId::generate(&code);

        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), code));

        let calls = Calls::builder().create_program(code_id.into_bytes(), [], []);

        let _ = init_constructor_with_value(Scheme::direct(calls), 500);

        assert_total_dequeued(2 + 1);
        assert_init_success(2);
    })
}

#[test]
fn demo_constructor_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_constructor::{Arg, Calls, Scheme};

        let (_init_mid, constructor_id) = utils::init_constructor(Scheme::empty());

        let calls = Calls::builder()
            .source("source")
            .send_value("source", Arg::bytes("Hello, user!"), 100_000)
            .store_vec("message_id")
            .send("source", "message_id");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            100_000,
            false,
        ));

        let message_id = get_last_message_id();

        run_to_next_block(None);

        let message_id = MessageId::generate_outgoing(message_id, 0);

        let last_mail = maybe_any_last_message().expect("Element should be");
        assert_eq!(last_mail.payload_bytes(), message_id.as_ref());

        let (first_mail, _bn) = {
            let res = MailboxOf::<Test>::remove(USER_1, message_id);
            assert!(res.is_ok());
            res.expect("was asserted previously")
        };

        assert_eq!(first_mail.value(), 100_000);
        assert_eq!(first_mail.payload_bytes(), b"Hello, user!");

        let calls = Calls::builder().panic("I just panic every time");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let message_id = get_last_message_id();

        run_to_next_block(None);

        let error_text = "panicked with 'I just panic every time'".to_owned();

        assert_failed(message_id, AssertFailedError::Panic(error_text));

        let reply = maybe_any_last_message().expect("Should be");
        assert_eq!(reply.id(), MessageId::generate_reply(message_id));
        assert_eq!(
            reply.reply_code().expect("Should be"),
            ReplyCode::error(SimpleExecutionError::UserspacePanic)
        )
    });
}

#[test]
fn demo_constructor_value_eq() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_constructor::{Arg, Calls, Scheme};

        let (_init_mid, constructor_id) = utils::init_constructor(Scheme::empty());

        let calls = Calls::builder()
            .value_as_vec("value")
            .bytes_eq("bool", "value", 100_000u128.encode())
            .if_else(
                "bool",
                Calls::builder().reply(Arg::bytes("Eq")),
                Calls::builder().reply(Arg::bytes("Ne")),
            );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            100_000,
            false,
        ));

        run_to_next_block(None);

        let last_mail = maybe_any_last_message().expect("Element should be");
        assert_eq!(last_mail.payload_bytes(), b"Eq");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            constructor_id,
            calls.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        let last_mail = maybe_any_last_message().expect("Element should be");
        assert_eq!(last_mail.payload_bytes(), b"Ne");
    });
}

#[test]
fn demo_constructor_is_demo_ping() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_constructor::{Arg, Calls, Scheme};

        let ping = Arg::bytes("PING");
        let pong = Arg::bytes("PONG");

        let ping_branch = Calls::builder().send("source", pong);
        let noop_branch = Calls::builder().noop();

        let init = Calls::builder().reply(ping.clone());

        let handle = Calls::builder()
            .source("source")
            .load("payload")
            .bytes_eq("is_ping", "payload", ping)
            .if_else("is_ping", ping_branch, noop_branch);

        let handle_reply = Calls::builder().panic("I don't like replies");

        let scheme = Scheme::predefined(init, handle, handle_reply, Default::default());

        // checking init
        let (_init_mid, constructor_id) = utils::init_constructor(scheme);

        let init_reply = maybe_any_last_message().expect("Element should be");
        assert_eq!(init_reply.payload_bytes(), b"PING");

        let mut message_id_to_reply = None;

        // checking handle twice
        for _ in 0..2 {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                constructor_id,
                b"PING".to_vec(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));

            run_to_next_block(None);

            let last_mail = maybe_any_last_message().expect("Element should be");
            assert_eq!(last_mail.payload_bytes(), b"PONG");
            message_id_to_reply = Some(last_mail.id());
        }

        let message_id_to_reply = message_id_to_reply.expect("Should be");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id_to_reply,
            vec![],
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let reply_id = get_last_message_id();

        run_to_next_block(None);

        // we don't assert fail reason since no error reply sent on reply,
        // but message id has stamp in MessagesDispatched event.
        let status = dispatch_status(reply_id).expect("Not found in `MessagesDispatched`");
        assert_eq!(status, DispatchStatus::Failed);
    });
}

#[test]
fn test_reply_to_terminated_program() {
    use demo_constructor::demo_exit_init;

    init_logger();
    new_test_ext().execute_with(|| {
        let (original_message_id, _program_id) =
            submit_constructor_with_args(USER_1, DEFAULT_SALT, demo_exit_init::scheme(true), 0);

        let mail_id = MessageId::generate_outgoing(original_message_id, 0);

        run_to_block(2, None);

        // Check mail in Mailbox
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 1);

        // Send reply
        let reply_call = crate::mock::RuntimeCall::Gear(crate::Call::<Test>::send_reply {
            reply_to_id: mail_id,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: 10_000_000,
            value: 0,
            keep_alive: false,
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
            0,
            false,
        ));

        let prog = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(utils::is_active(prog));

        let auto_reply = maybe_last_message(USER_1).expect("Should be");
        assert!(auto_reply.details().is_some());
        assert!(auto_reply.payload_bytes().is_empty());
        assert_eq!(
            auto_reply.reply_code().expect("Should be"),
            ReplyCode::Success(SuccessReplyReason::Auto)
        );

        System::reset_events();

        for _ in 0..delay {
            assert!(maybe_last_message(USER_1).is_none());
            run_to_next_block(None);
        }

        assert!(!MailboxOf::<Test>::is_empty(&USER_1));
        assert_eq!(
            maybe_last_message(USER_1)
                .expect("Event should be")
                .payload_bytes(),
            b"Delayed hello!".encode()
        );
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
            0,
            false,
        ));

        let prog = utils::get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(prog));

        assert!(maybe_last_message(USER_1).is_some());

        // This message will go into waitlist.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            prog,
            // Non zero size payload to trigger other demos repr case.
            vec![0],
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
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
            0,
            false,
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
    use demo_mul_by_const::WASM_BINARY as MUL_CONST_WASM_BINARY;
    use demo_waiting_proxy::WASM_BINARY as WAITING_PROXY_WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let program_id = generate_program_id(MUL_CONST_WASM_BINARY, b"program");
        let wrapper_id = generate_program_id(WAITING_PROXY_WASM_BINARY, b"salt");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            MUL_CONST_WASM_BINARY.to_vec(),
            b"program".to_vec(),
            50_u64.encode(),
            10_000_000_000,
            0,
            false,
        ));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITING_PROXY_WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            (<[u8; 32]>::from(program_id), 0u64).encode(),
            10_000_000_000,
            0,
            false,
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
            true,
        )
        .expect("Failed to get gas spent");

        assert!(gas_reserved >= gas_to_spend);

        run_to_block(4, None);

        // A message is sent to a waiting proxy program that passes execution
        // on to another program while keeping the `value`.
        // The overall gas expenditure is `gas_to_spend`. The message gas limit
        // is set to be just enough to cover this amount.
        // The sender's account has enough funds for both gas and `value`,
        // therefore expecting the message to be processed successfully.
        // Expected outcome: the sender's balance has decreased by the
        // (`gas_to_spend` + `value`).

        let user_initial_balance = Balances::free_balance(USER_1);

        assert_eq!(user_balance_before_calculating, user_initial_balance);
        // Zero because no message added into mailbox.
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);
        assert!(MailboxOf::<Test>::is_empty(&USER_1));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            wrapper_id,
            payload,
            gas_reserved,
            value,
            false,
        ));

        let gas_to_spend = gas_price(gas_to_spend);
        let gas_reserved = gas_price(gas_reserved);
        let reserved_balance = gas_reserved + value;

        assert_balance(
            USER_1,
            user_initial_balance - reserved_balance,
            reserved_balance,
        );
        run_to_block(5, None);

        assert!(MailboxOf::<Test>::is_empty(&USER_1));
        assert_balance(USER_1, user_initial_balance - gas_to_spend - value, 0u128);
    });
}

#[test]
fn free_storage_hold_on_scheduler_overwhelm() {
    use demo_constructor::{Scheme, demo_value_sender::TestData};

    init_logger();
    new_test_ext().execute_with(|| {
        let (_init_mid, sender) = init_constructor(Scheme::empty());

        let data = TestData::gasful(20_000, 0);

        let mb_cost = CostsPerBlockOf::<Test>::mailbox();
        let reserve_for = CostsPerBlockOf::<Test>::reserve_for();

        let user_1_balance = Balances::free_balance(USER_1);
        assert_eq!(GearBank::<Test>::account_total(&USER_1), 0);

        let user_2_balance = Balances::free_balance(USER_2);
        assert_eq!(GearBank::<Test>::account_total(&USER_2), 0);

        let prog_balance = Balances::free_balance(sender.cast::<AccountId>());
        assert_eq!(GearBank::<Test>::account_total(&sender.cast()), 0);

        let (_, gas_info) = utils::calculate_handle_and_send_with_extra(
            USER_1,
            sender,
            data.request(USER_2.into_origin()).encode(),
            Some(data.extra_gas),
            0,
        );

        utils::assert_balance(
            USER_1,
            user_1_balance - gas_price(gas_info.min_limit + data.extra_gas),
            gas_price(gas_info.min_limit + data.extra_gas),
        );
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));

        run_to_next_block(None);

        let hold_bound =
            HoldBoundBuilder::<Test>::new(StorageType::Mailbox).maximum_for(data.gas_limit_to_send);

        let expected_duration =
            BlockNumberFor::<Test>::saturated_from(data.gas_limit_to_send / mb_cost) - reserve_for;

        assert_eq!(hold_bound.expected_duration(), expected_duration);

        utils::assert_balance(
            USER_1,
            user_1_balance - gas_price(gas_info.burned + data.gas_limit_to_send),
            gas_price(data.gas_limit_to_send),
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

        // Block which already can't be paid.
        run_to_next_block(None);

        let gas_totally_burned = gas_price(gas_info.burned + data.gas_limit_to_send);

        utils::assert_balance(USER_1, user_1_balance - gas_totally_burned, 0u128);
        utils::assert_balance(USER_2, user_2_balance, 0u128);
        utils::assert_balance(sender, prog_balance, 0u128);
        assert!(MailboxOf::<Test>::is_empty(&USER_2));
    });
}

#[test]
fn execution_over_blocks() {
    const MAX_BLOCK: u64 = 10_000_000_000;

    init_logger();

    let assert_last_message = |src: [u8; 32], count: u128| {
        use demo_calc_hash::verify_result;

        let last_message = maybe_last_message(USER_1).expect("Get last message failed.");
        let result =
            <[u8; 32]>::decode(&mut last_message.payload_bytes()).expect("Decode result failed");

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
            false,
        ));
        let in_one_block = get_last_program_id();

        run_to_next_block(Some(MAX_BLOCK));

        // estimate start cost
        let pkg = Package::new(times, src);
        let gas = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(in_one_block),
            pkg.encode(),
            0,
            true,
            true,
        )
        .expect("Failed to get gas spent");

        (init_gas.min_limit, gas.min_limit)
    };

    new_test_ext().execute_with(|| {
        use demo_calc_hash_in_one_block::{Package, WASM_BINARY};

        // We suppose that gas limit is less than gas allowance
        let block_gas_limit = MAX_BLOCK - 10_000;

        // Deploy demo-calc-hash-in-one-block.
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            5_000_000_000,
            0,
            false,
        ));
        let in_one_block = get_last_program_id();

        assert!(ProgramStorageOf::<Test>::program_exists(in_one_block));

        let src = [0; 32];

        let expected = 64;
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            in_one_block,
            Package::new(expected, src).encode(),
            block_gas_limit,
            0,
            false,
        ));

        run_to_next_block(Some(MAX_BLOCK));

        assert_last_message([0; 32], expected);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            in_one_block,
            Package::new(1_024, src).encode(),
            block_gas_limit,
            0,
            false,
        ));

        let message_id = get_last_message_id();
        run_to_next_block(Some(MAX_BLOCK));

        assert_failed(
            message_id,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );
    });

    new_test_ext().execute_with(|| {
        use demo_calc_hash::sha2_512_256;
        use demo_calc_hash_over_blocks::{Method, WASM_BINARY};
        let block_gas_limit = MAX_BLOCK;

        let (_, calc_threshold) = estimate_gas_per_calc();

        // deploy demo-calc-hash-over-blocks
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            calc_threshold.encode(),
            9_000_000_000,
            0,
            false,
        ));
        let over_blocks = get_last_program_id();

        assert!(ProgramStorageOf::<Test>::program_exists(over_blocks));

        let (src, id, expected) = ([0; 32], sha2_512_256(b"42"), 512);

        // trigger calculation
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            over_blocks,
            Method::Start { src, id, expected }.encode(),
            9_000_000_000,
            0,
            false,
        ));

        run_to_next_block(Some(MAX_BLOCK));

        let mut count = 0;
        loop {
            let lm = maybe_last_message(USER_1);

            if !(lm.is_none() || lm.unwrap().payload_bytes().is_empty()) {
                break;
            }

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                over_blocks,
                Method::Refuel(id).encode(),
                block_gas_limit,
                0,
                false,
            ));

            count += 1;
            run_to_next_block(Some(MAX_BLOCK));
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

        let err = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(prog_id),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect_err("Must return error");

        let trap = TrapExplanation::ForbiddenFunction;

        assert_eq!(err, format!("Program terminated with a trap: '{trap}'"));
    });
}

#[test]
fn waking_message_waiting_for_mx_lock_does_not_lead_to_deadlock() {
    use demo_waiter::{
        Command as WaiterCommand, LockContinuation, MxLockContinuation, WASM_BINARY as WAITER_WASM,
    };

    fn execution() {
        System::reset_events();

        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITER_WASM.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .expect("Failed to upload Waiter");
        let waiter_prog_id = get_last_program_id();
        run_to_next_block(None);

        let send_command_to_waiter = |command: WaiterCommand| {
            MailboxOf::<Test>::clear();
            Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                waiter_prog_id,
                command.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            )
            .unwrap_or_else(|_| panic!("Failed to send command {command:?} to Waiter"));
            let msg_id = get_last_message_id();
            let msg_block_number = System::block_number() + 1;
            run_to_next_block(None);
            (msg_id, msg_block_number)
        };

        let (lock_owner_msg_id, _lock_owner_msg_block_number) =
            send_command_to_waiter(WaiterCommand::MxLock(
                None,
                MxLockContinuation::General(LockContinuation::SleepFor(4)),
            ));

        let (lock_rival_1_msg_id, _) = send_command_to_waiter(WaiterCommand::MxLock(
            None,
            MxLockContinuation::General(LockContinuation::Nothing),
        ));

        send_command_to_waiter(WaiterCommand::WakeUp(lock_rival_1_msg_id.into()));

        let (lock_rival_2_msg_id, _) = send_command_to_waiter(WaiterCommand::MxLock(
            None,
            MxLockContinuation::General(LockContinuation::Nothing),
        ));

        assert!(WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &lock_owner_msg_id
        ));
        assert!(WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &lock_rival_1_msg_id
        ));
        assert!(WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &lock_rival_2_msg_id
        ));

        // Run for 1 block, so the lock owner wakes up after sleeping for 4 blocks,
        // releases the mutex so the lock rival 1 can acquire and release it for
        // the lock rival 2 to acquire it.
        run_for_blocks(1, None);

        assert_succeed(lock_owner_msg_id);
        assert_succeed(lock_rival_1_msg_id);
        assert_succeed(lock_rival_2_msg_id);
    }

    init_logger();
    new_test_ext().execute_with(execution);
}

#[test]
fn waking_message_waiting_for_rw_lock_does_not_lead_to_deadlock() {
    use demo_waiter::{
        Command as WaiterCommand, LockContinuation, RwLockContinuation, RwLockType,
        WASM_BINARY as WAITER_WASM,
    };

    fn execution() {
        System::reset_events();

        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITER_WASM.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .expect("Failed to upload Waiter");
        let waiter_prog_id = get_last_program_id();
        run_to_next_block(None);

        let send_command_to_waiter = |command: WaiterCommand| {
            MailboxOf::<Test>::clear();
            Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                waiter_prog_id,
                command.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            )
            .unwrap_or_else(|_| panic!("Failed to send command {command:?} to Waiter"));
            let msg_id = get_last_message_id();
            let msg_block_number = System::block_number() + 1;
            run_to_next_block(None);
            (msg_id, msg_block_number)
        };

        // For write lock
        {
            let (lock_owner_msg_id, _lock_owner_msg_block_number) =
                send_command_to_waiter(WaiterCommand::RwLock(
                    RwLockType::Read,
                    RwLockContinuation::General(LockContinuation::SleepFor(4)),
                ));

            let (lock_rival_1_msg_id, _) = send_command_to_waiter(WaiterCommand::RwLock(
                RwLockType::Write,
                RwLockContinuation::General(LockContinuation::Nothing),
            ));

            send_command_to_waiter(WaiterCommand::WakeUp(lock_rival_1_msg_id.into()));

            let (lock_rival_2_msg_id, _) = send_command_to_waiter(WaiterCommand::RwLock(
                RwLockType::Write,
                RwLockContinuation::General(LockContinuation::Nothing),
            ));

            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_owner_msg_id
            ));
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_rival_1_msg_id
            ));
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_rival_2_msg_id
            ));

            // Run for 1 block, so the lock owner wakes up after sleeping for 4 blocks,
            // releases the mutex so the lock rival 1 can acquire and release it for
            // the lock rival 2 to acquire it.
            run_for_blocks(1, None);

            assert_succeed(lock_owner_msg_id);
            assert_succeed(lock_rival_1_msg_id);
            assert_succeed(lock_rival_2_msg_id);
        }

        // For read lock
        {
            let (lock_owner_msg_id, _lock_owner_msg_block_number) =
                send_command_to_waiter(WaiterCommand::RwLock(
                    RwLockType::Write,
                    RwLockContinuation::General(LockContinuation::SleepFor(4)),
                ));

            let (lock_rival_1_msg_id, _) = send_command_to_waiter(WaiterCommand::RwLock(
                RwLockType::Read,
                RwLockContinuation::General(LockContinuation::Nothing),
            ));

            send_command_to_waiter(WaiterCommand::WakeUp(lock_rival_1_msg_id.into()));

            let (lock_rival_2_msg_id, _) = send_command_to_waiter(WaiterCommand::RwLock(
                RwLockType::Write,
                RwLockContinuation::General(LockContinuation::Nothing),
            ));

            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_owner_msg_id
            ));
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_rival_1_msg_id
            ));
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &lock_rival_2_msg_id
            ));

            // Run for 1 block, so the lock owner wakes up after sleeping for 4 blocks,
            // releases the mutex so the lock rival 1 can acquire and release it for
            // the lock rival 2 to acquire it.
            run_for_blocks(1, None);

            assert_succeed(lock_owner_msg_id);
            assert_succeed(lock_rival_1_msg_id);
            assert_succeed(lock_rival_2_msg_id);
        }
    }

    init_logger();
    new_test_ext().execute_with(execution);
}

#[test]
fn mx_lock_ownership_exceedance() {
    use demo_waiter::{
        Command as WaiterCommand, LockContinuation, MxLockContinuation, WASM_BINARY as WAITER_WASM,
    };

    const LOCK_HOLD_DURATION: u32 = 3;

    fn execution() {
        System::reset_events();

        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITER_WASM.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .expect("Failed to upload Waiter");
        let waiter_prog_id = get_last_program_id();
        run_to_next_block(None);

        // Helper functions (collapse the block)
        let (run_test_case, get_lock_ownership_exceeded_trap) = {
            let send_command_to_waiter = |command: WaiterCommand| {
                MailboxOf::<Test>::clear();
                Gear::send_message(
                    RuntimeOrigin::signed(USER_1),
                    waiter_prog_id,
                    command.encode(),
                    BlockGasLimitOf::<Test>::get(),
                    0,
                    false,
                )
                .unwrap_or_else(|_| panic!("Failed to send command {command:?} to Waiter"));
                let msg_id = get_last_message_id();
                let msg_block_number = System::block_number() + 1;
                run_to_next_block(None);
                (msg_id, msg_block_number)
            };

            let run_test_case =
                move |command: WaiterCommand,
                      run_for_blocks_before_lock_assert: u32,
                      assert_command_result: &dyn Fn(MessageId),
                      assert_lock_result: &dyn Fn(MessageId, MessageId)| {
                    let (command_msg_id, _) = send_command_to_waiter(command);

                    // Subtract 1 because sending command to waiter below adds 1 block
                    run_for_blocks(
                        (run_for_blocks_before_lock_assert - 1).saturated_into(),
                        None,
                    );

                    assert_command_result(command_msg_id);

                    let (lock_msg_id, _) = send_command_to_waiter(WaiterCommand::MxLock(
                        Some(1),
                        MxLockContinuation::General(LockContinuation::Nothing),
                    ));

                    assert_lock_result(command_msg_id, lock_msg_id);
                };

            let get_lock_ownership_exceeded_trap = |command_msg_id| {
                AssertFailedError::Panic(format!(
                    "panicked with 'Message 0x{} has exceeded lock ownership time'",
                    hex::encode(command_msg_id)
                ))
            };

            (run_test_case, get_lock_ownership_exceeded_trap)
        };

        // Msg1 acquires lock and goes into waitlist
        // Msg2 acquires the lock after Msg1's lock ownership time has exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::Wait),
            ),
            LOCK_HOLD_DURATION,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert_failed(
                    command_msg_id,
                    get_lock_ownership_exceeded_trap(command_msg_id),
                );
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and goes into waitlist
        // Msg2 fails to acquire the lock because Msg1's lock ownership time has not exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::Wait),
            ),
            LOCK_HOLD_DURATION - 1,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));
            },
        );

        // Msg1 acquires lock and goes into waitlist
        // Msg2 fails to acquire the lock at the first attempt because Msg1's lock ownership
        // time has not exceeded, but succeeds at the second one after Msg1's lock ownership
        // time has exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::Wait),
            ),
            LOCK_HOLD_DURATION - 1,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));

                run_for_blocks(1, None);
                assert_failed(
                    command_msg_id,
                    get_lock_ownership_exceeded_trap(command_msg_id),
                );
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and forgets its lock guard
        // Msg2 acquires the lock after Msg1's lock ownership time has exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::Forget),
            ),
            LOCK_HOLD_DURATION,
            &|command_msg_id| {
                assert_succeed(command_msg_id);
            },
            &|_command_msg_id, lock_msg_id| {
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and forgets its lock guard
        // Msg2 fails to acquire the lock because Msg1's lock ownership time has not exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::Forget),
            ),
            LOCK_HOLD_DURATION - 1,
            &|command_msg_id| {
                assert_succeed(command_msg_id);
            },
            &|_command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));
            },
        );

        // Msg1 acquires lock and goes into sleep for longer than its lock ownership time
        // Msg2 acquires the lock after Msg1's lock ownership time has exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::SleepFor(LOCK_HOLD_DURATION * 2)),
            ),
            LOCK_HOLD_DURATION,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert_failed(
                    command_msg_id,
                    get_lock_ownership_exceeded_trap(command_msg_id),
                );
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and goes into sleep for longer than its lock ownership time
        // Msg2 fails to acquire the lock because Msg1's lock ownership time has not exceeded
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION),
                MxLockContinuation::General(LockContinuation::SleepFor(LOCK_HOLD_DURATION * 2)),
            ),
            LOCK_HOLD_DURATION - 1,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));
            },
        );

        // Msg1 acquires lock and goes into sleep for shorter than its lock ownership time
        // Msg2 fails to acquire the lock because Msg1's lock ownership time has not exceeded,
        // but succeeds after Msg1 releases the lock after the sleep
        run_test_case(
            WaiterCommand::MxLock(
                Some(LOCK_HOLD_DURATION + 1),
                MxLockContinuation::General(LockContinuation::SleepFor(LOCK_HOLD_DURATION)),
            ),
            2,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));

                run_for_blocks(1, None);
                assert_succeed(command_msg_id);
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and tries to re-enter the same lock
        // Msg2 acquires the lock after Msg1's lock ownership time has exceeded
        run_test_case(
            WaiterCommand::MxLock(Some(LOCK_HOLD_DURATION), MxLockContinuation::Lock),
            LOCK_HOLD_DURATION,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert_failed(
                    command_msg_id,
                    get_lock_ownership_exceeded_trap(command_msg_id),
                );
                assert_succeed(lock_msg_id);
            },
        );

        // Msg1 acquires lock and tries to re-enter the same lock
        // Msg2 fails to acquire the lock because Msg1's lock ownership time has not exceeded
        run_test_case(
            WaiterCommand::MxLock(Some(LOCK_HOLD_DURATION), MxLockContinuation::Lock),
            LOCK_HOLD_DURATION - 1,
            &|command_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
            },
            &|command_msg_id, lock_msg_id| {
                assert!(WaitlistOf::<Test>::contains(
                    &waiter_prog_id,
                    &command_msg_id
                ));
                assert!(WaitlistOf::<Test>::contains(&waiter_prog_id, &lock_msg_id));
            },
        );
    }

    init_logger();
    new_test_ext().execute_with(execution);
}

#[test]
fn async_sleep_for() {
    use demo_waiter::{
        Command as WaiterCommand, SleepForWaitType as WaitType, WASM_BINARY as WAITER_WASM,
    };

    const SLEEP_FOR_BLOCKS: BlockNumber = 2;
    const LONGER_SLEEP_FOR_BLOCKS: BlockNumber = 3;

    init_logger();

    new_test_ext().execute_with(|| {
        System::reset_events();

        // Block 2
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WAITER_WASM.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .expect("Failed to upload Waiter");
        let waiter_prog_id = get_last_program_id();
        run_to_next_block(None);

        // Helper functions (collapse the block)
        let (send_command_to_waiter, assert_waiter_single_reply) = {
            let send_command_to_waiter = |command: WaiterCommand| {
                MailboxOf::<Test>::clear();
                Gear::send_message(
                    RuntimeOrigin::signed(USER_1),
                    waiter_prog_id,
                    command.encode(),
                    BlockGasLimitOf::<Test>::get(),
                    0,
                    false,
                )
                .unwrap_or_else(|_| panic!("Failed to send command {command:?} to Waiter"));
                let msg_id = get_last_message_id();
                let msg_block_number = System::block_number() + 1;
                run_to_next_block(None);
                (msg_id, msg_block_number)
            };

            let assert_waiter_single_reply = |expected_reply| {
                assert_eq!(
                    MailboxOf::<Test>::len(&USER_1),
                    1,
                    "Asserting Waiter reply {expected_reply}",
                );
                let waiter_reply = <String>::decode(&mut get_last_mail(USER_1).payload_bytes())
                    .expect("Failed to decode Waiter reply");
                assert_eq!(
                    waiter_reply, expected_reply,
                    "Asserting Waiter reply {expected_reply}",
                );
            };

            (send_command_to_waiter, assert_waiter_single_reply)
        };

        // Block 3
        let (sleep_for_msg_id, sleep_for_block_number) = send_command_to_waiter(
            WaiterCommand::SleepFor(vec![SLEEP_FOR_BLOCKS.saturated_into()], WaitType::All),
        );

        // Assert the program replied with a message before the sleep.
        // The message payload is a number of the block the program received
        // the SleepFor message in.
        assert_waiter_single_reply(format!(
            "Before the sleep at block: {sleep_for_block_number}",
        ));

        // Assert the SleepFor message is in the waitlist.
        assert!(WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &sleep_for_msg_id
        ));

        // Block 4
        send_command_to_waiter(WaiterCommand::WakeUp(sleep_for_msg_id.into()));

        // Assert there are no any replies yet.
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 0);

        // Assert the SleepFor message is still in the waitlist.
        assert!(WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &sleep_for_msg_id
        ));

        // Block 5
        run_to_next_block(None);

        // Assert the program replied with a message after the sleep.
        // The message payload is a number of the block the program
        // exited the delay, i.e. sleep_for_block_number + SLEEP_FOR_BLOCKS.
        assert_waiter_single_reply(format!(
            "After the sleep at block: {}",
            sleep_for_block_number + SLEEP_FOR_BLOCKS
        ));

        // Assert the SleepFor message is no longer in the waitlist.
        assert!(!WaitlistOf::<Test>::contains(
            &waiter_prog_id,
            &sleep_for_msg_id
        ));

        // let long_sleep = sleep_for(longer);
        // let short_sleep = sleep_for(shorter);
        // join!(long_sleep, short_sleep);
        {
            // Block 6
            let (sleep_for_msg_id, sleep_for_block_number) =
                send_command_to_waiter(WaiterCommand::SleepFor(
                    vec![
                        LONGER_SLEEP_FOR_BLOCKS.saturated_into(),
                        SLEEP_FOR_BLOCKS.saturated_into(),
                    ],
                    WaitType::All,
                ));
            // Clear the before sleep reply.
            MailboxOf::<Test>::clear();

            // Block 8
            run_for_blocks(SLEEP_FOR_BLOCKS, None);

            // Assert there are no any replies yet even though SLEEP_FOR_BLOCKS blocks
            // has just passed.
            assert_eq!(MailboxOf::<Test>::len(&USER_1), 0);

            // Assert the SleepFor message is still in the waitlist.
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));

            // Block 9
            run_to_next_block(None);

            // Assert the program replied with a message after the sleep.
            // The message payload is a number of the block the program
            // exited the delay, i.e. sleep_for_block_number + LONGER_SLEEP_FOR_BLOCKS.
            assert_waiter_single_reply(format!(
                "After the sleep at block: {}",
                sleep_for_block_number + LONGER_SLEEP_FOR_BLOCKS
            ));

            // Assert the SleepFor message is no longer in the waitlist.
            assert!(!WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));
        }

        // let short_sleep = sleep_for(shorter);
        // let long_sleep = sleep_for(longer);
        // join!(short_sleep, long_sleep);
        {
            // Block 10
            let (sleep_for_msg_id, sleep_for_block_number) =
                send_command_to_waiter(WaiterCommand::SleepFor(
                    vec![
                        LONGER_SLEEP_FOR_BLOCKS.saturated_into(),
                        SLEEP_FOR_BLOCKS.saturated_into(),
                    ],
                    WaitType::All,
                ));
            // Clear the before sleep reply.
            MailboxOf::<Test>::clear();

            // Block 12
            run_for_blocks(SLEEP_FOR_BLOCKS, None);

            // Assert there are no any replies yet even though SLEEP_FOR_BLOCKS blocks
            // has just passed.
            assert_eq!(MailboxOf::<Test>::len(&USER_1), 0);

            // Assert the SleepFor message is still in the waitlist.
            assert!(WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));

            // Block 13
            run_to_next_block(None);

            // Assert the program replied with a message after the sleep.
            // The message payload is a number of the block the program
            // exited the delay, i.e. sleep_for_block_number + LONGER_SLEEP_FOR_BLOCKS.
            assert_waiter_single_reply(format!(
                "After the sleep at block: {}",
                sleep_for_block_number + LONGER_SLEEP_FOR_BLOCKS
            ));

            // Assert the SleepFor message is no longer in the waitlist.
            assert!(!WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));
        }

        // let long_sleep = sleep_for(longer);
        // let short_sleep = sleep_for(shorter);
        // select!(short_sleep, long_sleep);
        {
            // Block 14
            let (sleep_for_msg_id, sleep_for_block_number) =
                send_command_to_waiter(WaiterCommand::SleepFor(
                    vec![
                        LONGER_SLEEP_FOR_BLOCKS.saturated_into(),
                        SLEEP_FOR_BLOCKS.saturated_into(),
                    ],
                    WaitType::Any,
                ));
            // Clear the before sleep reply.
            MailboxOf::<Test>::clear();

            // Block 16
            run_for_blocks(SLEEP_FOR_BLOCKS, None);

            // Assert the program replied with a message after the sleep.
            // The message payload is a number of the block the program
            // exited the delay, i.e. sleep_for_block_number + SLEEP_FOR_BLOCKS.
            assert_waiter_single_reply(format!(
                "After the sleep at block: {}",
                sleep_for_block_number + SLEEP_FOR_BLOCKS
            ));

            // Assert the SleepFor message is no longer in the waitlist.
            assert!(!WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));
        }

        // let short_sleep = sleep_for(shorter);
        // let long_sleep = sleep_for(longer);
        // select!(short_sleep, long_sleep);
        {
            // Block 17
            let (sleep_for_msg_id, sleep_for_block_number) =
                send_command_to_waiter(WaiterCommand::SleepFor(
                    vec![
                        LONGER_SLEEP_FOR_BLOCKS.saturated_into(),
                        SLEEP_FOR_BLOCKS.saturated_into(),
                    ],
                    WaitType::Any,
                ));
            // Clear the before sleep reply.
            MailboxOf::<Test>::clear();

            // Block 18
            run_for_blocks(SLEEP_FOR_BLOCKS, None);

            // Assert the program replied with a message after the sleep.
            // The message payload is a number of the block the program
            // exited the delay, i.e. sleep_for_block_number + SLEEP_FOR_BLOCKS.
            assert_waiter_single_reply(format!(
                "After the sleep at block: {}",
                sleep_for_block_number + SLEEP_FOR_BLOCKS
            ));

            // Assert the SleepFor message is no longer in the waitlist.
            assert!(!WaitlistOf::<Test>::contains(
                &waiter_prog_id,
                &sleep_for_msg_id
            ));
        }
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
            false,
        ));

        let pid = get_last_program_id();
        for kind in &[
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
                30_000_000_000u64,
                0,
                false,
            ));

            // check the message sent from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(Kind::decode(&mut last_mail.payload_bytes()), Ok(*kind));

            // reply to the message
            let message_id = last_mail.id();
            assert_ok!(Gear::send_reply(
                RuntimeOrigin::signed(USER_1),
                message_id,
                EMPTY_PAYLOAD.to_vec(),
                10_000_000_000u64,
                0,
                false,
            ));

            // check the reply from the program
            run_to_next_block(None);
            let last_mail = get_last_mail(USER_1);
            assert_eq!(last_mail.payload_bytes(), b"PONG");
            assert_ok!(Gear::claim_value(
                RuntimeOrigin::signed(USER_1),
                last_mail.id()
            ));
        }

        assert!(utils::is_active(pid));
    })
}

#[test]
fn test_async_program_creation() {
    use demo_async_tester::{Kind, WASM_BINARY};
    use demo_ping::WASM_BINARY as REPLIER;

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
            false,
        ));

        let pid = utils::get_last_program_id();

        // upload a replier.
        run_to_next_block(None);
        let code_id = CodeId::generate(REPLIER).into_bytes();
        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            REPLIER.into()
        ));

        // 1. create program from code id.
        run_to_next_block(None);
        let kind = Kind::CreateProgram(code_id.into());
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            kind.encode(),
            30_000_000_000u64,
            2 * get_ed(), // required to be able to create a program
            false,
        ));

        // verify the new created program has been initialized successfully.
        run_to_next_block(None);
        let last_mail = get_last_mail(USER_1);
        assert_eq!(last_mail.payload_bytes(), b"PONG");
        assert_init_success(2);

        // 2. create program from with gas code id.
        run_to_next_block(None);
        let kind = Kind::CreateProgramWithGas(code_id.into(), 10_000_000_000u64);
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            kind.encode(),
            30_000_000_000u64,
            0,
            false,
        ));

        // verify the new created program has been initialized successfully.
        run_to_next_block(None);
        let last_mail = get_last_mail(USER_1);
        assert_eq!(last_mail.payload_bytes(), b"PONG");
        assert_init_success(3);
    })
}
#[test]
fn program_generator_works() {
    use demo_program_generator::{CHILD_WAT, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(CHILD_WAT).to_bytes();
        let code_id = CodeId::generate(&code);

        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), code));

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            1000,
            false,
        ));

        let generator_id = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(generator_id));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            generator_id,
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let message_id = get_last_message_id();

        run_to_next_block(None);

        assert_succeed(message_id);
        let expected_salt = [b"salt_generator", message_id.as_ref(), &0u64.to_be_bytes()].concat();
        let expected_child_id = ActorId::generate_from_program(message_id, code_id, &expected_salt);
        assert!(ProgramStorageOf::<Test>::program_exists(expected_child_id))
    });
}

#[test]
fn wait_state_machine() {
    use demo_wait::WASM_BINARY;

    init_logger();

    let init = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let wait_id = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(wait_id));

        System::reset_events();

        wait_id
    };

    new_test_ext().execute_with(|| {
        let demo = init();

        let to_send = vec![b"FIRST".to_vec(), b"SECOND".to_vec(), b"THIRD".to_vec()];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::ReplyCode(ReplyCode::Success(SuccessReplyReason::Auto)),
            Assertion::ReplyCode(ReplyCode::Success(SuccessReplyReason::Auto)),
            Assertion::Payload(ids[0].as_ref().to_vec()),
            Assertion::ReplyCode(ReplyCode::Success(SuccessReplyReason::Auto)),
            Assertion::Payload(ids[1].as_ref().to_vec()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });
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
        let ed = get_ed();

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
            true,
        )
        .expect("calculate_gas_info failed");

        let program_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
        let metadata_cost = DbWeightOf::<Test>::get().reads(1).ref_time();
        // there is no execution so the values should be equal
        assert_eq!(min_limit, program_cost + metadata_cost);

        run_to_next_block(None);

        // there is no 'init' so memory pages and code don't get loaded and
        // no execution is performed at all and hence user was not charged for program execution.
        assert_eq!(
            balance_before,
            Balances::free_balance(USER_1) + gas_price(program_cost + metadata_cost) + ed
        );

        // this value is actually a constant in the wat.
        let locked_value = 1_000;
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &USER_1,
            &program_id.cast(),
            locked_value,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        let reply_to_id = get_last_mail(USER_1).id();

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Reply(reply_to_id, ReplyCode::Success(SuccessReplyReason::Manual)),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_eq!(min_limit, program_cost + metadata_cost);

        let balance_before = Balances::free_balance(USER_1);
        let reply_value = 1_500;
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000,
            reply_value,
            false,
        ));

        run_to_next_block(None);

        assert_eq!(
            balance_before - reply_value + locked_value,
            Balances::free_balance(USER_1) + gas_price(program_cost + metadata_cost)
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
            false,
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
            false,
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
            false,
        ));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_3),
            program_handle_id,
            EMPTY_PAYLOAD.to_vec(),
            1_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        let margin = balance_before - Balances::free_balance(USER_1);
        let margin_handle = balance_before_handle - Balances::free_balance(USER_3);

        assert!(margin < margin_handle);
    });
}

#[test]
fn invalid_memory_page_amount_rejected() {
    let incorrect_amount = code::MAX_WASM_PAGES_AMOUNT + 1;

    let wat = format!(
        r#"
            (module
                (import "env" "memory" (memory {incorrect_amount}))
                (export "init" (func $init))
                (func $init)
            )
        "#
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
                false,
            ),
            Error::<Test>::ProgramConstructionFailed
        );
    });
}

#[test]
fn test_reinstrumentation_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code_id = CodeId::generate(&ProgramCodeKind::Default.to_bytes());
        let pid = upload_program_default(USER_1, ProgramCodeKind::Default).unwrap();

        run_to_block(2, None);

        // check old version
        let _reset_guard = DynamicSchedule::mutate(|schedule| {
            let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id).unwrap();
            assert_eq!(
                code_metadata
                    .instruction_weights_version()
                    .expect("Failed to get instructions weight version"),
                schedule.instruction_weights.version
            );

            schedule.instruction_weights.version = 0xdeadbeef;
        });

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        // check new version
        let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id).unwrap();
        assert_eq!(
            code_metadata
                .instruction_weights_version()
                .expect("Failed to get instructions weight version"),
            0xdeadbeef
        );

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        // check new version stands still
        let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id).unwrap();
        assert_eq!(
            code_metadata
                .instruction_weights_version()
                .expect("Failed to get instructions weight version"),
            0xdeadbeef
        );
    })
}

#[test]
fn test_reinstrumentation_failure() {
    init_logger();
    new_test_ext().execute_with(|| {
        let code_id = CodeId::generate(&ProgramCodeKind::Default.to_bytes());
        let pid = upload_program_default(USER_1, ProgramCodeKind::Default).unwrap();

        run_to_block(2, None);

        let new_weights_version = 0xdeadbeef;

        let _reset_guard = DynamicSchedule::mutate(|schedule| {
            // Insert new original code to cause re-instrumentation failure.
            let wasm = ProgramCodeKind::Custom("(module)").to_bytes();
            <<Test as Config>::CodeStorage as CodeStorage>::OriginalCodeMap::insert(code_id, wasm);

            schedule.instruction_weights.version = new_weights_version;
        });

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            10_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        // Must be active even after re-instrumentation failure.
        let program = ProgramStorageOf::<Test>::get_program(pid).unwrap();
        assert!(program.is_active());

        // After message processing the code must have the new instrumentation version.
        let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id).unwrap();
        assert_eq!(
            code_metadata
                .instruction_weights_version()
                .expect("Failed to get instructions weight version"),
            new_weights_version
        );

        // Error reply must be returned with the reason of re-instrumentation failure.
        assert_failed(
            mid,
            ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::ReinstrumentationFailure,
            ),
        );
    })
}

#[test]
fn test_init_reinstrumentation_failure() {
    init_logger();

    new_test_ext().execute_with(|| {
        let code_id = CodeId::generate(&ProgramCodeKind::Default.to_bytes());
        let pid = upload_program_default(USER_1, ProgramCodeKind::Default).unwrap();

        let new_weights_version = 0xdeadbeef;

        let _reset_guard = DynamicSchedule::mutate(|schedule| {
            // Insert new original code to cause init re-instrumentation failure.
            let wasm = ProgramCodeKind::Custom("(module)").to_bytes();
            <<Test as Config>::CodeStorage as CodeStorage>::OriginalCodeMap::insert(code_id, wasm);

            schedule.instruction_weights.version = new_weights_version;
        });

        let mid = get_last_message_id();

        run_to_block(2, None);

        // Must be terminated after re-instrumentation failure, because it failed on init.
        let program = ProgramStorageOf::<Test>::get_program(pid).unwrap();
        assert!(program.is_terminated());

        // After message processing the code must have the new instrumentation version.
        let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id).unwrap();
        assert_eq!(
            code_metadata
                .instruction_weights_version()
                .expect("Failed to get instructions weight version"),
            new_weights_version
        );

        // Error reply must be returned with the reason of re-instrumentation failure.
        assert_failed(
            mid,
            ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::ReinstrumentationFailure,
            ),
        );
    })
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
            schedule.limits.data_segments_amount.into(),
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
            false,
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
            false,
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
                false,
            ));

            run_to_block(3, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 500);
            assert_eq!(msg.payload_bytes(), b"send_to_user");
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
                false,
            ));

            let mid = get_last_message_id();

            run_to_block(4, None);

            assert_succeed(mid);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 700);
            assert_eq!(msg.payload_bytes(), b"receive_from_program");
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
                false,
            ));

            run_to_block(5, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));

            run_to_block(6, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 600);
            assert_eq!(msg.payload_bytes(), b"send_to_user_delayed");
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
                false,
            ));

            let mid = get_last_message_id();

            run_to_block(7, None);

            assert!(MailboxOf::<Test>::is_empty(&USER_1));
            assert_succeed(mid);

            run_to_block(8, None);

            let msg = get_last_mail(USER_1);
            assert_eq!(msg.value(), 800);
            assert_eq!(msg.payload_bytes(), b"receive_from_program_delayed");
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
            false,
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        run_to_block(2, None);

        {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::ReplyToUser.encode(),
                30_000_000_000,
                1_000,
                false,
            ));

            run_to_block(3, None);

            let msg = maybe_last_message(USER_1).expect("Should be");
            assert_eq!(msg.value(), 900);
            assert_eq!(msg.payload_bytes(), b"reply_to_user");
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
                30_000_000_000,
                1_000,
                false,
            ));

            let mid = get_last_message_id();

            run_to_block(4, None);

            assert_succeed(mid);

            let msg = maybe_last_message(USER_1).expect("Should be");
            assert_eq!(msg.value(), 900);
            assert_eq!(msg.payload_bytes(), b"reply");
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::PanicInSignal.encode(),
            10_000_000_000,
            0,
            false,
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
        Gear::set_block_number(expiration - 1);

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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitWithReserveAmountAndPanic(1).encode(),
            10_000_000_000,
            0,
            false,
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
            0,
            false,
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
            2 + 1 // reply from program + reply to user because of panic +1 for auto generated replies
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let gas_for_program_read = DbWeightOf::<Test>::get().reads(1).ref_time();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitWithReserveAmountAndPanic(gas_for_program_read).encode(),
            10_000_000_000,
            0,
            false,
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
            0,
            false,
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
            2 + 1 // reply from program + reply to user because of panic +1 for auto generated replies
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
            12_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        let GasInfo {
            min_limit: gas_spent,
            ..
        } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            gas_spent,
            0,
            false,
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
        Gear::set_block_number(expiration - 1);

        System::reset_events();
        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let _mail_msg = maybe_last_message(USER_1).expect("Should be");
    });
}

#[test]
fn signal_run_out_of_gas_works() {
    test_signal_code_works(
        SimpleExecutionError::RanOutOfGas.into(),
        demo_signal_entry::HandleAction::OutOfGas,
    );
}

#[test]
fn signal_run_out_of_gas_memory_access_works() {
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    const GAS_LIMIT: u64 = 10_000_000_000;

    init_logger();
    new_test_ext().execute_with(|| {
        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            GAS_LIMIT,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(utils::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Save signal code to be compared with
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::SaveSignal(SimpleExecutionError::RanOutOfGas.into()).encode(),
            GAS_LIMIT,
            0,
            false,
        ));

        run_to_next_block(None);

        // Calculate gas limit for this action
        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            demo_signal_entry::HandleAction::MemoryAccess.encode(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        // Send the action to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            demo_signal_entry::HandleAction::MemoryAccess.encode(),
            min_limit - 1,
            0,
            false,
        ));

        let mid = get_last_message_id();

        // Assert that system reserve gas node is removed
        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        run_to_next_block(None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // Ensure that signal code sent is signal code we saved
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload_bytes(), true.encode());
    });
}

#[test]
fn signal_userspace_panic_works() {
    test_signal_code_works(
        SimpleExecutionError::UserspacePanic.into(),
        demo_signal_entry::HandleAction::Panic,
    );
}

#[test]
fn signal_backend_error_forbidden_action_works() {
    test_signal_code_works(
        SimpleExecutionError::BackendError.into(),
        demo_signal_entry::HandleAction::ForbiddenAction,
    );
}

#[test]
fn signal_backend_error_invalid_debug_works() {
    test_signal_code_works(
        SimpleExecutionError::BackendError.into(),
        demo_signal_entry::HandleAction::InvalidDebugCall,
    );
}

#[test]
fn signal_backend_error_unrecoverable_ext_works() {
    test_signal_code_works(
        SimpleExecutionError::BackendError.into(),
        demo_signal_entry::HandleAction::UnrecoverableExt,
    );
}

#[test]
fn signal_unreachable_instruction_works() {
    test_signal_code_works(
        SimpleExecutionError::UnreachableInstruction.into(),
        demo_signal_entry::HandleAction::UnreachableInstruction,
    );
}

#[test]
fn signal_unreachable_instruction_incorrect_free_works() {
    test_signal_code_works(
        SimpleExecutionError::UnreachableInstruction.into(),
        demo_signal_entry::HandleAction::IncorrectFree,
    );
}

#[test]
fn signal_memory_overflow_works() {
    test_signal_code_works(
        SimpleExecutionError::MemoryOverflow.into(),
        demo_signal_entry::HandleAction::ExceedMemory,
    );
}

#[test]
fn signal_removed_from_waitlist_works() {
    const GAS_LIMIT: u64 = 10_000_000_000;
    use demo_signal_entry::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            GAS_LIMIT,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        // Ensure that program is uploaded and initialized correctly
        assert!(utils::is_active(pid));
        assert!(Gear::is_initialized(pid));

        // Save signal code to be compared with
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::SaveSignal(SignalCode::RemovedFromWaitlist).encode(),
            GAS_LIMIT,
            0,
            false,
        ));

        run_to_next_block(None);

        // Send the action to trigger signal sending
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitWithoutSendingMessage.encode(),
            GAS_LIMIT,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        // Ensuring that gas is reserved
        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

        // Getting block number when waitlist expiration should happen
        let expiration = get_waitlist_expiration(mid);

        // Hack to fast spend blocks till expiration
        System::set_block_number(expiration - 1);
        Gear::set_block_number(expiration - 1);

        // Expiring that message
        run_to_next_block(None);

        // Ensure that signal code sent is signal code we saved
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload_bytes(), true.encode());
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
            false,
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
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Simple.encode(),
            10_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        let burned = gas_price(burned);
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::AcrossWaits.encode(),
            30_000_000_000,
            0,
            false,
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
                30_000_000_000,
                0,
                false,
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
            12_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Panic.encode(),
            13_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload_bytes(), b"handle_signal");
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Exit.encode(),
            10_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_succeed(mid);
        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch was not executed but `gr_exit` did
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 0);
        let msg = maybe_last_message(USER_1).expect("Should be");
        assert_eq!(msg.payload_bytes(), b"exit");
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndPanic.encode(),
            10_000_000_000,
            0,
            false,
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
            0,
            false,
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Wait.encode(),
            10_000_000_000,
            0,
            false,
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
        Gear::set_block_number(expiration - 1);

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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndExit.encode(),
            10_000_000_000,
            0,
            false,
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
            0,
            false,
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check `gr_exit` occurs
        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"wait_and_exit");
    });
}

#[test]
fn system_reservation_wait_and_reserve_with_panic_works() {
    use demo_signal_entry::{HandleAction, WAIT_AND_RESERVE_WITH_PANIC_GAS, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            USER_1.encode(),
            10_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::WaitAndReserveWithPanic.encode(),
            30_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_eq!(
            GasHandlerOf::<Test>::get_system_reserve(mid),
            Ok(WAIT_AND_RESERVE_WITH_PANIC_GAS)
        );

        let reply_to_id = get_last_mail(USER_1).id();
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply_to_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

        // check signal dispatch executed
        let mail_msg = get_last_mail(USER_1);
        assert_eq!(mail_msg.payload_bytes(), b"handle_signal");
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Accumulate.encode(),
            10_000_000_000,
            0,
            false,
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::ZeroReserve.encode(),
            10_000_000_000,
            0,
            false,
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
            InitAction::Normal(vec![
                // orphan reservation; will be removed automatically
                (50_000, 3),
                // must be cleared during `gr_exit`
                (25_000, 5),
            ])
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();
        let balance_rent_pool = Balances::free_balance(RENT_POOL);

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
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Unreserve.encode(),
            spent_gas,
            0,
            false,
        ));

        run_to_block(3, None);

        // gas unreserved manually
        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 2);

        let gas_reserved = gas_price(spent_gas);
        let reservation_amount = gas_price(RESERVATION_AMOUNT);
        let reservation_holding = 15 * gas_price(CostsPerBlockOf::<Test>::reservation());

        assert_eq!(
            Balances::free_balance(USER_1),
            user_initial_balance - gas_reserved + reservation_amount + reservation_holding
        );
        // reservation was held for one block so the rent pool should be increased accordingly
        assert_eq!(
            Balances::free_balance(RENT_POOL),
            balance_rent_pool + gas_price(CostsPerBlockOf::<Test>::reservation())
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
            &slot.finish.saturated_into(),
            &task
        ));

        // `gr_exit` occurs
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Exit.encode(),
            50_000_000_000,
            0,
            false,
        ));

        run_to_block(2 + 4, None);

        // check task was cleared after `gr_exit` happened
        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &slot.finish.saturated_into(),
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let message_id = get_last_mail(USER_1).id();

        assert!(!Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 1);

        let (reservation_id, slot) = map.iter().next().unwrap();
        let task = ScheduledTask::RemoveGasReservation(pid, *reservation_id);
        assert!(TaskPoolOf::<Test>::contains(
            &slot.finish.saturated_into(),
            &task
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            ReplyAction::Panic.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_block(3, None);

        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &slot.finish.saturated_into(),
            &task
        ));
        assert!(!Gear::is_initialized(pid));
        assert!(!utils::is_active(pid));
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let message_id = get_last_mail(USER_1).id();

        assert!(!Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        let map = get_reservation_map(pid).unwrap();
        assert_eq!(map.len(), 1);

        let (reservation_id, slot) = map.iter().next().unwrap();
        let task = ScheduledTask::RemoveGasReservation(pid, *reservation_id);
        assert!(TaskPoolOf::<Test>::contains(
            &slot.finish.saturated_into(),
            &task
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            message_id,
            ReplyAction::Exit.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_block(3, None);

        let map = get_reservation_map(pid);
        assert_eq!(map, None);
        assert!(!TaskPoolOf::<Test>::contains(
            &slot.finish.saturated_into(),
            &task
        ));
        assert!(!Gear::is_initialized(pid));
        assert!(!utils::is_active(pid));
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
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(2, None);

        assert_succeed(mid);
    });
}

#[test]
fn gas_reservations_fresh_reserve_unreserve() {
    use demo_reserve_gas::InitAction;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::FreshReserveUnreserve.encode(),
            10_000_000_000,
            0,
            false,
        ));
        let mid = get_last_message_id();

        run_to_block(2, None);

        assert_succeed(mid);
        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"fresh_reserve_unreserve");
    });
}

#[test]
fn gas_reservations_existing_reserve_unreserve() {
    use demo_reserve_gas::{HandleAction, InitAction};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_reserve_gas::WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitAction::Normal(vec![]).encode(),
            10_000_000_000,
            0,
            false,
        ));
        let mid = get_last_message_id();
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_succeed(mid);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::SendFromReservationAndUnreserve.encode(),
            10_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        assert_succeed(mid);
        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"existing_reserve_unreserve");
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
            30_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            30_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"my_handle_signal");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            30_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"my_handle_reply");
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::ForbiddenCallInSignal(USER_1.into_origin().into()).encode(),
            10_000_000_000,
            0,
            false,
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
        Gear::set_block_number(expiration - 1);

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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            HandleAction::Simple.encode(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Simple.encode(),
            min_limit,
            0,
            false,
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
            false,
        ));

        let pid = get_last_program_id();

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Wait.encode(),
            10_000_000_000,
            0,
            false,
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
            false,
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
        Gear::set_block_number(expiration - 1);

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
            12_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();
        let init_mid = get_last_message_id();

        run_to_block(2, None);

        assert!(utils::is_active(pid));
        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(init_mid));

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"init");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));

        let reply_mid = get_last_message_id();

        run_to_block(3, None);

        assert!(!Gear::is_initialized(pid));
        assert!(GasHandlerOf::<Test>::get_system_reserve(init_mid).is_err());
        assert!(GasHandlerOf::<Test>::get_system_reserve(reply_mid).is_err());
    });
}

#[test]
fn missing_block_tasks_handled() {
    init_logger();
    new_test_ext().execute_with(|| {
        // https://github.com/gear-tech/gear/pull/2404#pullrequestreview-1399996879
        // possible case described by @breathx:
        // block N contains no tasks, first missed block = None
        // block N+1 contains tasks, but block producer missed run_queue extrinsic or runtime upgrade occurs
        // block N+2 contains tasks and starts execute them because missed blocks = None so tasks from block N+1 lost forever
        const N: BlockNumber = 3;

        let pid =
            upload_program_default(USER_1, ProgramCodeKind::OutgoingWithValueInHandle).unwrap();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            100_000_000,
            1000,
            false,
        ));

        run_to_block(N - 1, None);

        let mid = get_last_message_id();
        let task = ScheduledTask::RemoveFromMailbox(USER_1, mid);
        TaskPoolOf::<Test>::add(N + 1, task).unwrap();

        assert!(MailboxOf::<Test>::contains(&USER_1, &mid));

        // insert task
        run_to_block(N, None);

        // task was inserted
        assert!(TaskPoolOf::<Test>::contains(&(N + 1), &task));
        assert!(MailboxOf::<Test>::contains(&USER_1, &mid));

        // task must be skipped in this block
        run_to_block_maybe_with_queue(N + 1, Some(0), None);
        System::reset_events(); // remove `QueueProcessingReverted` event to run to block N + 2

        // task could be processed in N + 1 block but `Gear::run` extrinsic have been skipped
        assert!(TaskPoolOf::<Test>::contains(&(N + 1), &task));
        assert!(MailboxOf::<Test>::contains(&USER_1, &mid));

        // continue to process task from previous block
        run_to_block(N + 2, None);

        // task have been processed
        assert!(!TaskPoolOf::<Test>::contains(&(N + 1), &task));
        // so message should be removed from mailbox
        assert!(!MailboxOf::<Test>::contains(&USER_1, &mid));
    });
}

#[test]
fn async_does_not_duplicate_sync() {
    use demo_ping::WASM_BINARY as PING_BINARY;
    use demo_sync_duplicate::WASM_BINARY as SYNC_DUPLICATE_BINARY;

    init_logger();

    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            SYNC_DUPLICATE_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let sync = get_last_program_id();

        run_to_next_block(None);

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            sync,
            b"async".to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        let mail = maybe_any_last_message().expect("Element should be");
        assert_eq!(mail.destination().into_origin(), USER_1.into_origin());
        assert_eq!(mail.payload_bytes(), 1i32.to_le_bytes());
    })
}

#[test]
fn state_rollback() {
    use demo_state_rollback::WASM_BINARY;

    init_logger();

    let init = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let rollback = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(rollback));

        System::reset_events();

        rollback
    };

    let panic_bytes = b"panic".to_vec();
    let leave_bytes = b"leave".to_vec();

    // state-rollback
    new_test_ext().execute_with(|| {
        let program = init();

        let to_send = vec![0.encode(), panic_bytes, 1.encode()];
        send_payloads(USER_1, program, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(None::<Vec<u8>>.encode()),
            Assertion::Payload(Some(0.encode()).encode()),
            Assertion::ReplyCode(ReplyCode::error(SimpleExecutionError::UserspacePanic)),
            Assertion::Payload(Some(0.encode()).encode()),
            Assertion::Payload(Some(1.encode()).encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // state-saving
    new_test_ext().execute_with(|| {
        let program = init();

        let to_send = vec![0.encode(), leave_bytes.clone(), 1.encode()];
        send_payloads(USER_1, program, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(None::<Vec<u8>>.encode()),
            Assertion::Payload(Some(0.encode()).encode()),
            Assertion::Payload(Some(0.encode()).encode()),
            Assertion::Payload(Some(leave_bytes.clone()).encode()),
            Assertion::Payload(Some(leave_bytes).encode()),
            Assertion::Payload(Some(1.encode()).encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    })
}

#[test]
fn rw_lock_works() {
    use demo_ping::WASM_BINARY as PING_BINARY;
    use demo_rwlock::{Command, WASM_BINARY};

    init_logger();

    let upload = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let prog_id = get_last_program_id();

        run_to_next_block(None);
        System::reset_events();

        prog_id
    };

    // RwLock wide
    new_test_ext().execute_with(|| {
        let rwlock = upload();

        let to_send = [
            Command::Get,
            Command::Inc,
            Command::Get,
            Command::PingGet,
            Command::IncPing,
        ]
        .iter()
        .map(Encode::encode)
        .collect();
        send_payloads(USER_1, rwlock, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(0u32.encode()),
            Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
            Assertion::Payload(1u32.encode()),
            Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
            Assertion::Payload(2u32.encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // RwLock read while writing
    new_test_ext().execute_with(|| {
        let rwlock = upload();

        let to_send = [Command::IncPing, Command::Get]
            .iter()
            .map(Encode::encode)
            .collect();
        send_payloads(USER_1, rwlock, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
            Assertion::Payload(1u32.encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // RwLock write while reading
    new_test_ext().execute_with(|| {
        let rwlock = upload();

        let to_send = [Command::GetPing, Command::Get, Command::Inc]
            .iter()
            .map(Encode::encode)
            .collect();
        send_payloads(USER_1, rwlock, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(0i32.encode()),
            Assertion::Payload(0i32.encode()),
            Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // RwLock deadlock
    new_test_ext().execute_with(|| {
        let rwlock = upload();

        let to_send = [
            Default::default(), // None-Command
            Command::Get.encode(),
        ]
        .into_iter()
        .collect();
        send_payloads(USER_1, rwlock, to_send);
        run_to_next_block(None);

        let to_assert = vec![];
        assert_responses_to_user(USER_1, to_assert);
    });

    // RwLock check readers
    new_test_ext().execute_with(|| {
        let rwlock = upload();

        let to_send = vec![Command::CheckReaders.encode()];
        send_payloads(USER_1, rwlock, to_send);
        run_to_next_block(None);

        let to_assert = vec![Assertion::Payload(0i32.encode())];
        assert_responses_to_user(USER_1, to_assert);
    });
}

#[test]
fn async_works() {
    use demo_async::{Command, WASM_BINARY};
    use demo_ping::WASM_BINARY as PING_BINARY;

    init_logger();

    let upload = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let prog_id = get_last_program_id();

        run_to_next_block(None);
        System::reset_events();

        prog_id
    };

    // Common async scenario
    new_test_ext().execute_with(|| {
        let demo = upload();

        let to_send = vec![Command::Common.encode()];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let to_assert = vec![Assertion::Payload(ids[0].encode())];
        assert_responses_to_user(USER_1, to_assert);
    });

    // Mutex scenario
    new_test_ext().execute_with(|| {
        let demo = upload();

        let to_send = vec![Command::Mutex.encode(); 2];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let to_assert = (0..4)
            .map(|i| Assertion::Payload(ids[i / 2].encode()))
            .collect();
        assert_responses_to_user(USER_1, to_assert);
    });
}

#[test]
fn futures_unordered() {
    use demo_async::WASM_BINARY as DEMO_ASYNC_BINARY;
    use demo_futures_unordered::{Command, WASM_BINARY};
    use demo_ping::WASM_BINARY as PING_BINARY;

    init_logger();

    let upload = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            DEMO_ASYNC_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let demo_async = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            (demo_async, ping).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let prog_id = get_last_program_id();

        run_to_next_block(None);
        System::reset_events();

        prog_id
    };

    // FuturesUnordered
    new_test_ext().execute_with(|| {
        let demo = upload();

        let to_send = vec![Command::Unordered.encode()];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(b"PONG".to_vec()),
            Assertion::Payload(MessageId::generate_outgoing(ids[0], 0).encode()),
            Assertion::Payload(ids[0].encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // Select
    new_test_ext().execute_with(|| {
        let demo = upload();

        let to_send = vec![Command::Select.encode()];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let to_assert = vec![
            Assertion::Payload(b"PONG".to_vec()),
            Assertion::Payload(ids[0].encode()),
        ];
        assert_responses_to_user(USER_1, to_assert);
    });

    // Join
    new_test_ext().execute_with(|| {
        let demo = upload();

        let to_send = vec![Command::Join.encode()];
        let ids = send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let mut res = MessageId::generate_outgoing(ids[0], 0).encode();
        res.append(&mut b"PONG".to_vec());

        let to_assert = vec![Assertion::Payload(res), Assertion::Payload(ids[0].encode())];
        assert_responses_to_user(USER_1, to_assert);
    });
}

#[test]
fn async_recursion() {
    use demo_async_recursion::WASM_BINARY;
    use demo_ping::WASM_BINARY as PING_BINARY;

    init_logger();

    let upload = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping.encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let prog_id = get_last_program_id();

        run_to_next_block(None);
        System::reset_events();

        prog_id
    };

    new_test_ext().execute_with(|| {
        let demo = upload();
        let arg = 100i32;

        let to_send = vec![arg.encode()];
        send_payloads(USER_1, demo, to_send);
        run_to_next_block(None);

        let mut to_assert = (1..=arg)
            .rev()
            .filter(|&i| i % 4 == 0)
            .map(|i| Assertion::Payload(i.encode()))
            .collect::<Vec<_>>();
        to_assert.insert(
            to_assert.len() - 1,
            Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
        );
        assert_responses_to_user(USER_1, to_assert);
    });
}

#[test]
fn async_init() {
    use demo_async_init::{InputArgs, WASM_BINARY};
    use demo_ping::WASM_BINARY as PING_BINARY;

    init_logger();

    let upload = || {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_3),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping = get_last_program_id();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs::from_two(ping, ping).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        get_last_program_id()
    };

    new_test_ext().execute_with(|| {
        // upload and send to uninitialized program
        let demo = upload();
        send_payloads(USER_1, demo, vec![b"PING".to_vec()]);

        run_to_next_block(None);

        assert_responses_to_user(
            USER_1,
            vec![
                // `demo_async_init` sent error reply on "PING" message
                Assertion::ReplyCode(
                    ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::Uninitialized)
                        .into(),
                ),
                // `demo_async_init`'s `init` was successful
                Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
            ],
        );

        print_gear_events();

        System::reset_events();

        // send to already initialized program
        send_payloads(USER_1, demo, vec![b"PING".to_vec()]);

        run_to_next_block(None);

        assert_responses_to_user(
            USER_1,
            vec![
                // `demo_async_init` sent amount of responses it got from `demo_ping`
                Assertion::Payload(2u8.encode()),
            ],
        );
    });
}

#[test]
fn wake_after_exit() {
    use demo_custom::{InitMessage, WASM_BINARY};
    use demo_ping::WASM_BINARY as PING_BINARY;

    init_logger();

    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_3),
            PING_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Default::default(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let ping: [u8; 32] = get_last_program_id().into();

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InitMessage::WakeAfterExit(ping.into()).encode(),
            BlockGasLimitOf::<Test>::get(),
            1000,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        // Execution after wake must be skipped, so status must be NotExecuted.
        assert_not_executed(mid);
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
            false,
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
            false,
        )
        .expect("Failed to upload program");

        let message_id = get_last_message_id();

        run_to_block(2, None);
        assert_last_dequeued(1);

        assert_failed(
            message_id,
            ErrorReplyReason::Execution(SimpleExecutionError::UnreachableInstruction),
        );
    });
}

/// Check that random works and it's changing on next epoch.
#[test]
fn check_random_works() {
    use blake2::{Blake2b, Digest, digest::typenum::U32};

    /// BLAKE2b-256 hasher state.
    type Blake2b256 = Blake2b<U32>;

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
            false,
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
                false,
            ));

            let output: ([u8; 32], BlockNumber) =
                <Test as Config>::Randomness::random(get_last_message_id().as_ref());

            random_data.push([[0; 32], output.0].concat());
            run_to_block(System::block_number() + 1, None);
        });

        assert_eq!(random_data.len(), MailboxOf::<Test>::len(&USER_1));

        let mut sorted_mailbox: Vec<(UserStoredMessage, Interval<BlockNumber>)> =
            MailboxOf::<Test>::iter_key(USER_1).collect();
        sorted_mailbox.sort_by(|a, b| a.1.finish.cmp(&b.1.finish));

        sorted_mailbox
            .iter()
            .zip(random_data.iter())
            .for_each(|((msg, _bn), random_data)| {
                let mut ctx = Blake2b256::new();
                ctx.update(random_data);
                let expected = ctx.finalize();

                assert_eq!(expected.as_slice(), msg.payload_bytes());
            });

        // assert_last_dequeued(1);
        // println!("{:?}", res);
        // assert_eq!(blake2b(32, &[], &output.0.encode()).as_bytes(), res.payload());
    });
}

#[test]
fn reply_with_small_non_zero_gas() {
    use demo_proxy_relay::{RelayCall, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let gas_limit = 1;
        assert!(gas_limit < <Test as Config>::MailboxThreshold::get());

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            RelayCall::RereplyWithGas(gas_limit).encode(),
            50_000_000_000,
            0u128,
            false,
        ));

        let proxy = utils::get_last_program_id();

        run_to_next_block(None);
        assert!(utils::is_active(proxy));

        let payload = b"it works";

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            payload.to_vec(),
            DEFAULT_GAS_LIMIT * 10,
            0,
            false,
        ));

        let message_id = utils::get_last_message_id();

        run_to_next_block(None);
        assert_succeed(message_id);
        assert_eq!(
            maybe_last_message(USER_1)
                .expect("Should be")
                .payload_bytes(),
            payload
        );
    });
}

#[test]
fn replies_denied_in_handle_reply() {
    use demo_proxy::{InputArgs, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            InputArgs {
                destination: USER_1.into_origin().into()
            }
            .encode(),
            50_000_000_000,
            0u128,
            false,
        ));

        let proxy = utils::get_last_program_id();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy,
            vec![],
            50_000_000_000,
            0,
            false,
        ));

        let message_id = get_last_message_id();

        run_to_next_block(None);
        assert!(utils::is_active(proxy));
        assert_succeed(message_id);

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            get_last_mail(USER_1).id(),
            vec![],
            50_000_000_000,
            0,
            false,
        ));

        let reply_id = get_last_message_id();

        run_to_next_block(None);

        // we don't assert fail reason since no error reply sent on reply,
        // but message id has stamp in MessagesDispatched event.
        let status = dispatch_status(reply_id).expect("Not found in `MessagesDispatched`");
        assert_eq!(status, DispatchStatus::Failed);
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
                    0u128,
                    false,
                )
                .is_ok(),
                "{}",
                label
            );

            let proxy = utils::get_last_program_id();

            run_to_next_block(None);

            assert!(utils::is_active(proxy), "{}", label);

            assert!(
                Gear::send_message(
                    RuntimeOrigin::signed(source),
                    proxy,
                    payload.to_vec(),
                    DEFAULT_GAS_LIMIT * 10,
                    0,
                    false,
                )
                .is_ok(),
                "{}",
                label
            );

            // To clear auto reply on init message.
            System::reset_events();

            run_to_next_block(None);

            let received = System::events().into_iter().fold(0, |r, e| match e.event {
                MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) => {
                    let Expected { user, payload } = &expected[r];

                    if message.destination().into_origin() == user.into_origin() {
                        assert_eq!(message.payload_bytes(), payload, "{label}");
                        r + 1
                    } else {
                        r
                    }
                }
                _ => r,
            });

            assert_eq!(received, expected.len(), "{label}");
        };

        new_test_ext().execute_with(execute);
    };

    let payload = b"Hi, USER_2! Ping USER_3.";

    let pairs = vec![
        (
            RelayCall::ResendPush(vec![
                // "Hi, USER_2!"
                ResendPushData {
                    destination: USER_2.cast(),
                    start: None,
                    end: Some((10, true)),
                },
            ]),
            Expected {
                user: USER_2,
                payload: payload[..11].to_vec(),
            },
        ),
        (
            RelayCall::ResendPush(vec![
                // the same but end index specified in another way
                ResendPushData {
                    destination: USER_2.cast(),
                    start: None,
                    end: Some((11, false)),
                },
            ]),
            Expected {
                user: USER_2,
                payload: payload[..11].to_vec(),
            },
        ),
        (
            RelayCall::ResendPush(vec![
                // "Ping USER_3."
                ResendPushData {
                    destination: USER_3.cast(),
                    start: Some(12),
                    end: None,
                },
            ]),
            Expected {
                user: USER_3,
                payload: payload[12..].to_vec(),
            },
        ),
        (
            RelayCall::ResendPush(vec![
                // invalid range
                ResendPushData {
                    destination: USER_3.cast(),
                    start: Some(2),
                    end: Some((0, true)),
                },
            ]),
            Expected {
                user: USER_3,
                payload: vec![],
            },
        ),
    ];

    for (call, expectation) in pairs {
        test(call, payload, vec![expectation]);
    }

    test(
        RelayCall::Resend(USER_3.cast()),
        payload,
        vec![Expected {
            user: USER_3,
            payload: payload.to_vec(),
        }],
    );
    test(
        RelayCall::ResendWithGas(USER_3.cast(), 50_000),
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
        assert!(matches!(
            Code::try_new(
                ProgramCodeKind::Custom(wat).to_bytes(),
                1,
                |_| CustomConstantCostRules::default(),
                None,
                None,
            ),
            Err(CodeError::Export(ExportError::InvalidExportFnSignature(0)))
        ));
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::MemoryOverflow),
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::UnreachableInstruction),
        );
    });
}

#[test]
fn free_range_oob_error() {
    const WAT: &str = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "free_range" (func $free_range (param i32) (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        ;; free impossible and non-existing range
        i32.const 0x0
        i32.const 0xffffff
        call $free_range
        i32.const 0x0
        i32.ne
        if
            unreachable
        end
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
            10_000_000_000_u64,
            0,
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid));
        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::UnreachableInstruction),
        );
    });
}

#[test]
fn free_range_invalid_range_error() {
    const WAT: &str = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "free_range" (func $free_range (param i32) (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        ;; free invalid range (start > end)
        i32.const 0x55
        i32.const 0x2
        call $free_range
        i32.const 0x1 ;; we expect an error
        i32.ne
        if
            unreachable
        end
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);
        assert!(!Gear::is_terminated(pid));
        assert_succeed(mid);
    });
}

#[test]
fn free_range_success() {
    const WAT: &str = r#"
(module
    (import "env" "memory" (memory 1))
    (import "env" "alloc" (func $alloc (param i32) (result i32)))
    (import "env" "free" (func $free (param i32) (result i32)))
    (import "env" "free_range" (func $free_range (param i32) (param i32) (result i32)))
    (export "init" (func $init))
    (func $init
        ;; allocate 4 pages
        i32.const 0x4
        call $alloc

        i32.const 1
        i32.ne
        if
            unreachable
        end

        ;; free one page in range
        i32.const 0x2
        call $free

        i32.const 0
        i32.ne
        if
            unreachable
        end

        ;; free range with one missing page
        i32.const 0x1
        i32.const 0x4
        call $free_range
        i32.const 0x0
        i32.ne
        if
            unreachable
        end
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
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_succeed(mid);
        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));
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

#[test]
fn calculate_gas_fails_when_calculation_limit_exceeded() {
    use demo_reserve_gas::{HandleAction as Command, InitAction as Init, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            Init::Normal(vec![]).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .map(|_| get_last_program_id())
        .expect("Program uploading failed");

        run_to_next_block(None);

        // Make reservations exceeding calculation gas limit of RUNTIME_API_BLOCK_LIMITS_COUNT (6) blocks.
        for _ in 0..=RUNTIME_API_BLOCK_LIMITS_COUNT {
            Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                // 96% of block gas limit
                Command::AddReservationToList(BlockGasLimitOf::<Test>::get() / 100 * 96, 10)
                    .encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            )
            .expect("Making reservation failed");
        }

        run_to_next_block(None);

        let gas_info_result = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            Command::ConsumeReservationsFromList.encode(),
            0,
            true,
            true,
        );

        assert!(gas_info_result.is_err());
        assert_eq!(gas_info_result.unwrap_err(), ALLOWANCE_LIMIT_ERR);

        // ok result when we use custom multiplier
        let gas_info_result = Gear::calculate_gas_info_impl(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            BlockGasLimitOf::<Test>::get(),
            Command::ConsumeReservationsFromList.encode(),
            0,
            true,
            false,
            Some(64),
        );

        assert!(gas_info_result.is_ok());
    });
}

#[test]
fn reservation_manager() {
    use demo_reservation_manager::{Action, WASM_BINARY};
    use utils::Assertion;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        )
        .map(|_| get_last_program_id())
        .expect("Program uploading failed");

        run_to_next_block(None);

        fn scenario(pid: ActorId, payload: Action, expected: Vec<Assertion>) {
            System::reset_events();

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                payload.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));

            run_to_next_block(None);

            assert_responses_to_user(USER_1, expected);
        }

        // Try unreserve 100 gas when there's no reservations.
        scenario(
            pid,
            Action::SendMessageFromReservation { gas_amount: 100 },
            vec![Assertion::ReplyCode(ReplyCode::error(
                SimpleExecutionError::UserspacePanic,
            ))],
        );
        // Reserve 10_000 gas.
        scenario(
            pid,
            Action::Reserve {
                amount: 10_000,
                duration: 100,
            },
            vec![Assertion::ReplyCode(SuccessReplyReason::Auto.into())],
        );
        // Try to unreserve 50_000 gas.
        scenario(
            pid,
            Action::SendMessageFromReservation { gas_amount: 50_000 },
            vec![Assertion::ReplyCode(ReplyCode::error(
                SimpleExecutionError::UserspacePanic,
            ))],
        );
        // Try to unreserve 8_000 gas.
        scenario(
            pid,
            Action::SendMessageFromReservation { gas_amount: 8_000 },
            vec![
                // auto reply
                Assertion::ReplyCode(SuccessReplyReason::Auto.into()),
                // message with empty payload. not reply!
                Assertion::Payload(vec![]),
            ],
        );
        // Try to unreserve 8_000 gas again.
        scenario(
            pid,
            Action::SendMessageFromReservation { gas_amount: 8_000 },
            vec![Assertion::ReplyCode(ReplyCode::error(
                SimpleExecutionError::UserspacePanic,
            ))],
        );
    });
}

#[test]
fn send_gasless_message_works() {
    init_logger();

    let minimal_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let user1_initial_balance = Balances::free_balance(USER_1);
        let user2_initial_balance = Balances::free_balance(USER_2);
        let ed = get_ed();

        // No gas has been created initially
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        let program_id = {
            let res = upload_program_default(USER_1, ProgramCodeKind::Default);
            assert_ok!(res);
            res.expect("submit result was asserted")
        };

        // Test 1: USER_2 sends a gasless message to the program (intending to use a voucher).
        // Expect failure because USER_2 has no voucher.
        assert_noop!(
            GearVoucher::call(
                RuntimeOrigin::signed(USER_2),
                12345.into_origin().cast(),
                PrepaidCall::SendMessage {
                    destination: program_id,
                    payload: EMPTY_PAYLOAD.to_vec(),
                    gas_limit: DEFAULT_GAS_LIMIT,
                    value: 0,
                    keep_alive: false,
                }
            ),
            pallet_gear_voucher::Error::<Test>::InexistentVoucher
        );

        // USER_1 as the program owner issues a voucher for USER_2 enough to send a message
        assert_ok!(GearVoucher::issue(
            RuntimeOrigin::signed(USER_1),
            USER_2,
            gas_price(DEFAULT_GAS_LIMIT),
            Some([program_id].into()),
            false,
            100,
        ));

        // Balances check
        // USER_1 can spend up to 2 default messages worth of gas (submit program and issue voucher)
        let user1_potential_msgs_spends = gas_price(2 * DEFAULT_GAS_LIMIT);
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_initial_balance - user1_potential_msgs_spends - ed
        );

        // Clear messages from the queue to refund unused gas
        run_to_block(2, None);

        // Balance check
        // Voucher has been issued, but not used yet, so funds should be still in the respective account
        let voucher_id = utils::get_last_voucher_id();
        assert_eq!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>()),
            gas_price(DEFAULT_GAS_LIMIT)
        );

        // Test 2: USER_2 sends a gasless message to the program (intending to use a voucher).
        // Now that voucher is issued, the message should be sent successfully.
        assert_ok!(GearVoucher::call(
            RuntimeOrigin::signed(USER_2),
            voucher_id,
            PrepaidCall::SendMessage {
                destination: program_id,
                payload: EMPTY_PAYLOAD.to_vec(),
                gas_limit: DEFAULT_GAS_LIMIT,
                value: 1_000_000,
                keep_alive: false,
            }
        ));

        // Balances check
        // USER_2 as a voucher holder can send one message completely free of charge
        // The value in message, however, is still offset against the USER_2's own balance
        let user2_potential_msgs_spends = 1_000_000_u128;
        assert_eq!(
            Balances::free_balance(USER_2),
            user2_initial_balance - user2_potential_msgs_spends
        );
        // Instead, the gas has been paid from the voucher
        assert_eq!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>()),
            0_u128
        );

        // Run the queue processing to figure out the actual gas burned
        let remaining_weight = 300_000_000;
        run_to_block(3, Some(remaining_weight));

        let actual_gas_burned =
            remaining_weight - minimal_weight.ref_time() - GasAllowanceOf::<Test>::get();
        assert_ne!(actual_gas_burned, 0);

        // Check that the gas leftover has been returned to the voucher
        assert_eq!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>()),
            gas_price(DEFAULT_GAS_LIMIT) - gas_price(actual_gas_burned)
        );

        // USER_2 total balance has been reduced by the value in the message
        assert_eq!(
            Balances::total_balance(&USER_2),
            user2_initial_balance - user2_potential_msgs_spends
        );

        // No gas has got stuck in the system
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);
    });
}

#[test]
fn send_gasless_reply_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // USER_2 uploads a program and sends message to it which leads to
        // USER_1 having a message in his mailbox.
        // caution: runs to block 2
        let reply_to_id = setup_mailbox_test_state(USER_2);

        let prog_id = generate_program_id(
            &ProgramCodeKind::OutgoingWithValueInHandle.to_bytes(),
            DEFAULT_SALT.as_ref(),
        );

        // Top up program's account balance with some funds
        CurrencyOf::<Test>::resolve_creating(
            &prog_id.cast(),
            CurrencyOf::<Test>::issue(2_000_u128),
        );

        // USER_2 issues a voucher for USER_1 enough to send a reply
        assert_ok!(GearVoucher::issue(
            RuntimeOrigin::signed(USER_2),
            USER_1,
            gas_price(DEFAULT_GAS_LIMIT),
            Some([prog_id].into()),
            false,
            100,
        ));
        let voucher_id = utils::get_last_voucher_id();

        run_to_block(3, None);

        // Balance check
        assert_eq!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>()),
            gas_price(DEFAULT_GAS_LIMIT)
        );

        // USER_1 sends a gasless reply using a voucher
        let gas_limit = 10_000_000_u64;
        assert_ok!(GearVoucher::call(
            RuntimeOrigin::signed(USER_1),
            voucher_id,
            PrepaidCall::SendReply {
                reply_to_id,
                payload: EMPTY_PAYLOAD.to_vec(),
                gas_limit,
                value: 1_000,
                keep_alive: false,
            }
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

        // Balances check before processing queue
        assert_eq!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>()),
            gas_price(DEFAULT_GAS_LIMIT.saturating_sub(gas_limit))
        );

        run_to_block(4, None);
        // Ensure that some gas leftover has been returned to the voucher account
        assert!(
            Balances::free_balance(voucher_id.cast::<AccountIdOf<Test>>())
                > gas_price(DEFAULT_GAS_LIMIT.saturating_sub(gas_limit))
        );
    })
}

/// Tests whether calling `gr_read` 2 times returns same result.
/// Test purpose is to check, that payload is given back to the
/// message.
#[test]
fn double_read_works() {
    use demo_constructor::{Calls, Scheme};

    init_logger();
    new_test_ext().execute_with(|| {
        let noop_branch = Calls::builder().noop();
        let panic_branch = Calls::builder().panic("Read payloads aren't equal");
        let handle = Calls::builder()
            .load("read1")
            .load("read2")
            .bytes_eq("is_eq", "read1", "read2")
            .if_else("is_eq", noop_branch, panic_branch);
        let predefined_scheme = Scheme::predefined(
            Default::default(),
            handle,
            Default::default(),
            Default::default(),
        );

        let (_, pid) = utils::init_constructor(predefined_scheme);

        // Resetting events to check the result of the last message.
        System::reset_events();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            b"PAYLOAD".to_vec(),
            BlockGasLimitOf::<Test>::get(),
            100_000,
            false,
        ));

        run_to_next_block(None);

        assert_responses_to_user(
            USER_1,
            vec![Assertion::ReplyCode(SuccessReplyReason::Auto.into())],
        );
    });
}

/// Tests gas allowance exceed handling.
/// More precisely, it checks one property:
/// no context data is stored within previously
/// executed message when gas allowance exceed
/// happened.
#[test]
fn test_gas_allowance_exceed_no_context() {
    use crate::QueueProcessingOf;
    use common::storage::{Counted, Queue, Toggler};

    init_logger();
    new_test_ext().execute_with(|| {
        let wat = r#"
            (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_reply" (func $reply (param i32 i32 i32 i32)))
            (export "handle" (func $handle))
            (func $handle
                (call $reply (i32.const 0) (i32.const 32) (i32.const 10) (i32.const 333))
                (loop (br 0))
            )
        )"#;

        let pid = upload_program_default(USER_1, ProgramCodeKind::Custom(wat))
            .expect("failed uploading program");
        run_to_next_block(None);

        assert_ok!(send_default_message(USER_1, pid));
        let mid = get_last_message_id();
        // Setting to 100 million the gas allowance ends faster than gas limit
        run_to_next_block(Some(100_000_000));

        // Execution is denied after requeue
        assert!(QueueProcessingOf::<Test>::denied());

        // Low level check, that no execution context is saved after gas allowance exceeded error
        assert_eq!(QueueOf::<Test>::len(), 1);
        let msg = QueueOf::<Test>::dequeue()
            .ok()
            .flatten()
            .expect("must be message after requeue");
        assert_eq!(msg.id(), mid);
        assert!(msg.context().is_none());
        QueueOf::<Test>::requeue(msg).expect("requeue failed");

        // There should be now enough gas allowance, so the message is executed
        // and execution ends with `GasLimitExceeded`.
        run_to_next_block(None);

        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::RanOutOfGas),
        );
        assert_last_dequeued(1);
    })
}

/// Does pretty same test as `test_gas_allowance_exceed_no_context`,
/// but this time executed message will have non zero context.
#[test]
fn test_gas_allowance_exceed_with_context() {
    use crate::QueueProcessingOf;
    use common::storage::*;
    use demo_constructor::{Arg, Calls, Scheme};

    init_logger();

    let process_task_weight = mock::get_min_weight();

    new_test_ext().execute_with(|| {
        let call_wait_key = "call_wait";

        // Initialize a program and set `call_wait' value to `true`.
        let init = Calls::builder().source("user1").bool(call_wait_key, true);
        let (_, pid) = utils::init_constructor(Scheme::direct(init));

        let execute = |calls: Calls, allowance: Option<u64>| {
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                calls.encode(),
                BlockGasLimitOf::<Test>::get(),
                0,
                false,
            ));
            let msg_id = get_last_message_id();
            run_to_next_block(allowance);

            msg_id
        };

        // If `call_wait` is true, we execute a `wait`. Otherwise, we `reply` and `send`.
        // It's intended that the message will be executed several times.
        // That's to perform checks of the msg execution context when message
        // is in queue after wake (so it has some context), and after gas allowance exceeded
        // error (so the context remains unchanged).
        let wait_branch = Calls::builder().wait();
        let skip_wait_branch = Calls::builder().noop();
        let handle1 = Calls::builder()
            .if_else(
                Arg::Get(call_wait_key.to_string()),
                wait_branch,
                skip_wait_branch,
            )
            .reply(b"random_message".to_vec())
            .send("user1", b"another_random_message".to_vec());
        let handle1_mid = execute(handle1.clone(), None);

        // Check it waits.
        assert!(WaitlistOf::<Test>::contains(&pid, &handle1_mid));
        assert_eq!(WaitlistOf::<Test>::len(&pid), 1);

        // Taking the context for the check later.
        let handle1_ctx = WaitlistOf::<Test>::iter_key(pid)
            .next()
            .and_then(|(m, _)| m.context().clone());
        assert!(handle1_ctx.is_some());

        // This will set `call_wait` to false, wake the message with `handle1_mid` id.
        // We set the weight to such a value, so only message with `handle2`
        // payload is executed, That will allow us to reproduce the case in
        // `test_gas_allowance_exceed_no_context` test, but with message having
        // context already set.
        let handle2 = Calls::builder()
            .bool(call_wait_key, false)
            .wake(<[u8; 32]>::from(handle1_mid));
        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            handle2.encode(),
            0,
            true,
            true,
        )
        .expect("failed getting gas info");
        // We add `process_task_weight` as block running requires not only executing message, but
        // some other read/writes. By adding such small value we guarantee that
        // message with `handle2` payload is executed, but message with `handle1_mid`
        // id will not reach the executor.
        execute(
            handle2,
            Some(gas_info.min_limit + process_task_weight.ref_time()),
        );

        assert_last_dequeued(1);
        assert!(QueueProcessingOf::<Test>::denied());

        // Now we calculate a required for the execution of the `handle1_mid` message.
        // The queue processing is denied from the previous execution, now allowing it,
        // to calculate gas properly.
        QueueProcessingOf::<Test>::allow();
        let gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            handle1.encode(),
            0,
            true,
            true,
        )
        .expect("failed getting gas info");

        // Trigger gas allowance exceeded error while executing only `handle1_mid`.
        // With such gas allowance we are sure, that `reply` call from `handle1` calls set
        // is called successfully executed, but there's not enough allowance to end the
        // execution of the message.
        run_to_next_block(Some(gas_info.min_limit - 100_000));

        // Execution is denied after requeue.
        assert!(QueueProcessingOf::<Test>::denied());

        // Low level check, that no information on reply sent is saved in the execution
        // context after gas allowance exceeded error.
        assert_eq!(QueueOf::<Test>::len(), 1);
        let msg = QueueOf::<Test>::dequeue()
            .ok()
            .flatten()
            .expect("must be message after requeue");
        assert_eq!(msg.id(), handle1_mid);
        assert_eq!(msg.context(), &handle1_ctx);
        QueueOf::<Test>::requeue(msg).expect("requeue failed");

        run_to_next_block(None);
        assert_succeed(handle1_mid);
    })
}

/// Test that if a message is addressed to a terminated program (sent from program),
/// then no panic occurs and the message is not executed.
#[test]
fn test_send_to_terminated_from_program() {
    use demo_constructor::{Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let user_1_bytes = USER_1.into_origin().to_fixed_bytes();

        // Dies in init
        let init = Calls::builder().panic("Die in init");
        // "Bomb" in case after refactoring runtime we accidentally allow terminated programs to be executed.
        let handle = Calls::builder().send(user_1_bytes, b"REPLY_FROM_DEAD".to_vec());

        assert_ok!(Gear::upload_program(
            // Using `USER_2` not to pollute `USER_1` mailbox to make test easier.
            RuntimeOrigin::signed(USER_2),
            WASM_BINARY.to_vec(),
            b"salt1".to_vec(),
            Scheme::predefined(init, handle, Calls::default(), Calls::default()).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let pid_terminated = utils::get_last_program_id();

        // Check `pid_terminated` exists as an active program.
        assert!(utils::is_active(pid_terminated));

        // Sends in handle message to the dead program
        let handle = Calls::builder().send(pid_terminated.into_bytes(), []);
        // Sends to USER_1 the error reply from the dead program
        let handle_reply = Calls::builder()
            .reply_code("err_reply")
            .send(user_1_bytes, "err_reply");
        let (_, proxy_pid) = utils::submit_constructor_with_args(
            // Using `USER_2` not to pollute `USER_1` mailbox to make test easier.
            USER_2,
            b"salt2",
            Scheme::predefined(Calls::default(), handle, handle_reply, Calls::default()),
            0,
        );

        run_to_next_block(None);

        assert!(Gear::is_terminated(pid_terminated));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            proxy_pid,
            EMPTY_PAYLOAD.to_vec(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        run_to_next_block(None);

        // No panic occurred.
        // Need to check, that user has message in the mailbox with error reply (`InactiveProgram`).
        // Also check that user hasn't received anything from the dead program.
        let mut mails_from_proxy_iter = MailboxOf::<Test>::iter_key(USER_1)
            .filter_map(|(msg, _)| (msg.source() == proxy_pid).then_some(msg));
        let mail_from_proxy = mails_from_proxy_iter
            .next()
            .expect("internal error: no message from proxy");
        assert_eq!(
            mail_from_proxy.payload_bytes().to_vec(),
            ReplyCode::Error(ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::InitializationFailure
            ))
            .encode()
        );
        assert_eq!(mails_from_proxy_iter.next(), None);

        let mails_from_terminated_count = MailboxOf::<Test>::iter_key(USER_1)
            .filter(|(msg, _)| msg.source() == pid_terminated)
            .count();
        assert_eq!(mails_from_terminated_count, 0);
    })
}

#[test]
fn remove_from_waitlist_after_exit_reply() {
    use demo_constructor::demo_wait_init_exit_reply;

    init_logger();

    new_test_ext().execute_with(|| {
        let (init_mid, program_id) = init_constructor(demo_wait_init_exit_reply::scheme());

        assert!(!Gear::is_initialized(program_id));
        assert!(utils::is_active(program_id));

        run_to_next_block(None);

        let reply = maybe_last_message(USER_1).unwrap();
        let (waited_mid, remove_from_waitlist_block) = get_last_message_waited();
        assert_eq!(init_mid, waited_mid);

        run_to_next_block(None);

        assert!(TaskPoolOf::<Test>::contains(
            &remove_from_waitlist_block,
            &ScheduledTask::RemoveFromWaitlist(program_id, init_mid)
        ));

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            reply.id(),
            vec![],
            1_500_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        assert!(Gear::is_exited(program_id));
        assert!(!TaskPoolOf::<Test>::contains(
            &remove_from_waitlist_block,
            &ScheduledTask::RemoveFromWaitlist(program_id, init_mid)
        ));

        System::set_block_number(remove_from_waitlist_block - 1);
        Gear::set_block_number(remove_from_waitlist_block - 1);

        run_to_next_block(None);
    })
}

// currently we don't support WASM reference types
#[test]
fn wasm_ref_types_doesnt_work() {
    const WAT: &str = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "init" (func $init))
        (elem declare func $init)
        (func $init
            ref.func $init
            call $test
        )
        (func $test (param funcref))
    )
    "#;

    init_logger();
    new_test_ext().execute_with(|| {
        let _pid = upload_program_default(USER_1, ProgramCodeKind::Custom(WAT)).unwrap_err();
    });
}

/// Test that the `Gear::run()` extrinsic can only run once per block,
/// even if somehow included in a block multiple times.
#[test]
fn gear_run_only_runs_once_per_block() {
    use frame_support::{
        dispatch::RawOrigin,
        traits::{OnFinalize, OnInitialize},
    };

    fn init_block(bn: BlockNumberFor<Test>) {
        System::set_block_number(bn);
        GasAllowanceOf::<Test>::put(1_000_000_000);
        Gear::on_initialize(bn);
    }

    init_logger();
    new_test_ext().execute_with(|| {
        init_block(2);
        assert_ok!(Gear::run(RawOrigin::None.into(), None,));
        // Second run in a block is not allowed
        assert_noop!(
            Gear::run(RawOrigin::None.into(), None,),
            Error::<Test>::GearRunAlreadyInBlock
        );
        Gear::on_finalize(2);

        // Everything goes back to normal in the next block
        init_block(3);
        assert_ok!(Gear::run(RawOrigin::None.into(), None,));
    })
}

/// Test that the Gear internal block numbering is consistent.
#[test]
fn gear_block_number_math_adds_up() {
    init_logger();
    new_test_ext().execute_with(|| {
        run_to_block(100, None);
        assert_eq!(Gear::block_number(), 100);

        run_to_block_maybe_with_queue(120, None, None);
        assert_eq!(System::block_number(), 120);
        assert_eq!(Gear::block_number(), 100);

        System::reset_events();
        run_to_block(150, None);
        assert_eq!(System::block_number(), 150);
        assert_eq!(Gear::block_number(), 130);
    })
}

#[test]
fn test_gas_info_of_terminated_program() {
    use demo_constructor::{Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        // Dies in init
        let init_dead = Calls::builder().panic("Die in init");
        let handle_dead = Calls::builder().panic("Called after being terminated!");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"salt1".to_vec(),
            Scheme::predefined(init_dead, handle_dead, Calls::default(), Calls::default()).encode(),
            BlockGasLimitOf::<Test>::get(),
            0,
            false,
        ));

        let pid_dead = utils::get_last_program_id();

        // Sends in handle message do dead program
        let handle_proxy = Calls::builder().send(pid_dead.into_bytes(), []);
        let (_, proxy_pid) = utils::submit_constructor_with_args(
            USER_1,
            b"salt2",
            Scheme::predefined(
                Calls::default(),
                handle_proxy,
                Calls::default(),
                Calls::default(),
            ),
            0,
        );

        run_to_next_block(None);

        let _gas_info = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(proxy_pid),
            EMPTY_PAYLOAD.to_vec(),
            0,
            true,
            true,
        )
        .expect("failed getting gas info");
    })
}

#[test]
fn test_handle_signal_wait() {
    use demo_constructor::{Arg, Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let init = Calls::builder().bool("first", true);
        let handle = Calls::builder().if_else(
            Arg::get("first"),
            Calls::builder()
                .bool("first", false)
                .system_reserve_gas(10_000_000_000)
                .wait_for(1),
            Calls::builder().panic(None),
        );
        let handle_signal = Calls::builder().wait();

        let scheme = Scheme::predefined(init, handle, Default::default(), handle_signal);

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            scheme.encode(),
            100_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(pid));
        assert!(Gear::is_initialized(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));
        assert!(WaitlistOf::<Test>::contains(&pid, &mid));

        run_to_next_block(None);

        let signal_mid = MessageId::generate_signal(mid);
        assert!(WaitlistOf::<Test>::contains(&pid, &signal_mid));

        let (mid, block) = get_last_message_waited();

        assert_eq!(mid, signal_mid);

        System::set_block_number(block - 1);
        Gear::set_block_number(block - 1);
        run_to_next_block(None);

        assert!(!WaitlistOf::<Test>::contains(&pid, &signal_mid));

        assert_total_dequeued(4);
    });
}

#[test]
fn test_constructor_if_else() {
    use demo_constructor::{Arg, Call, Calls, Scheme, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        let init = Calls::builder().bool("switch", false);
        let handle = Calls::builder()
            .if_else(
                Arg::get("switch"),
                Calls::builder().add_call(Call::Bool(false)),
                Calls::builder().add_call(Call::Bool(true)),
            )
            .store("switch")
            .if_else(
                Arg::get("switch"),
                Calls::builder().wait_for(1),
                Calls::builder().exit(<[u8; 32]>::from(USER_1.into_origin())),
            );

        let scheme = Scheme::predefined(init, handle, Default::default(), Default::default());

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            scheme.encode(),
            100_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();

        run_to_next_block(None);

        assert!(utils::is_active(pid));
        assert!(Gear::is_initialized(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.to_vec(),
            100_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        let task = ScheduledTask::WakeMessage(pid, mid);

        assert!(WaitlistOf::<Test>::contains(&pid, &mid));
        assert!(TaskPoolOf::<Test>::contains(
            &(Gear::block_number() + 1),
            &task
        ));

        run_to_next_block(None);

        assert!(!WaitlistOf::<Test>::contains(&pid, &mid));
    });
}

#[test]
fn calculate_gas_wait() {
    use demo_constructor::{Calls, Scheme};

    init_logger();
    new_test_ext().execute_with(|| {
        let waiter = upload_program_default(USER_1, ProgramCodeKind::Custom(WAITER_WAT))
            .expect("submit result was asserted");

        let (_init_mid, sender) =
            submit_constructor_with_args(USER_1, DEFAULT_SALT, Scheme::empty(), 0);

        run_to_next_block(None);

        assert!(Gear::is_initialized(waiter));
        assert!(Gear::is_initialized(sender));

        let calls = Calls::builder().send(waiter.into_bytes(), []);

        let allow_other_panics = true;
        let allow_skip_zero_replies = true;
        let GasInfo { burned, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(sender),
            calls.encode(),
            0,
            allow_other_panics,
            allow_skip_zero_replies,
        )
        .expect("calculate_gas_info failed");

        let cost = CostsPerBlockOf::<Test>::waitlist();
        let GasInfo {
            min_limit: limit_no_rent,
            ..
        } = Gear::run_with_ext_copy(|| {
            Gear::calculate_gas_info_impl(
                USER_1.into_origin(),
                HandleKind::Handle(sender),
                burned + cost - 1,
                calls.encode(),
                0,
                allow_other_panics,
                allow_skip_zero_replies,
                None,
            )
        })
        .expect("calculate_gas_info failed");

        let GasInfo { min_limit, .. } = Gear::run_with_ext_copy(|| {
            Gear::calculate_gas_info_impl(
                USER_1.into_origin(),
                HandleKind::Handle(sender),
                burned + 1_000_000 * cost,
                calls.encode(),
                0,
                allow_other_panics,
                allow_skip_zero_replies,
                None,
            )
        })
        .expect("calculate_gas_info failed");

        // 'wait' syscall greedily consumes all available gas so
        // calculated limits should not be equal
        assert!(min_limit > limit_no_rent);
    });
}

#[test]
fn critical_hook_works() {
    use demo_async_critical::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            10_000_000_000,
            0,
            false,
        ));
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Simple.encode(),
            12_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let (waited, _) = get_last_message_waited();
        assert_eq!(mid, waited);
        assert_eq!(dispatch_status(mid), None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"for_reply");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        assert_succeed(mid);
        assert_eq!(MailboxOf::<Test>::iter_key(USER_1).count(), 0);
    });
}

#[test]
fn critical_hook_with_panic() {
    use demo_async_critical::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            12_000_000_000,
            0,
            false,
        ));
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::Panic.encode(),
            15_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"for_reply");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            12_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::UserspacePanic),
        );

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"critical");
    });
}

#[test]
fn critical_hook_in_handle_reply() {
    use demo_async_critical::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            10_000_000_000,
            0,
            false,
        ));
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::InHandleReply.encode(),
            12_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"for_reply");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(4, None);

        assert_eq!(MailboxOf::<Test>::iter_key(USER_1).last(), None);
        let status = dispatch_status(mid);
        assert_eq!(status, Some(DispatchStatus::Failed));
    });
}

#[test]
fn critical_hook_in_handle_signal() {
    use demo_async_critical::{HandleAction, WASM_BINARY};

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            10_000_000_000,
            0,
            false,
        ));
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            HandleAction::InHandleSignal.encode(),
            12_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_block(3, None);

        let msg = get_last_mail(USER_1);
        assert_eq!(msg.payload_bytes(), b"for_reply");

        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            msg.id(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_block(4, None);

        assert_eq!(MailboxOf::<Test>::iter_key(USER_1).last(), None);
        let signal_msg_id = MessageId::generate_signal(mid);
        let status = dispatch_status(signal_msg_id);
        assert_eq!(status, Some(DispatchStatus::Failed));
    });
}

#[test]
fn handle_reply_hook() {
    use demo_async_reply_hook::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        // Upload program
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            vec![],
            10_000_000_000,
            0,
            false,
        ));
        let pid = get_last_program_id();

        run_to_block(2, None);

        assert!(Gear::is_initialized(pid));
        assert!(utils::is_active(pid));

        // Init conversation
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            EMPTY_PAYLOAD.encode(),
            20_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        let messages = MailboxOf::<Test>::iter_key(USER_1).map(|(msg, _bn)| msg);

        let mut timeout_msg_id = None;

        for msg in messages {
            match msg.payload_bytes() {
                b"for_reply_1" => {
                    // Reply to the first message
                    assert_ok!(Gear::send_reply(
                        RuntimeOrigin::signed(USER_1),
                        msg.id(),
                        [1].to_vec(),
                        1_000_000_000,
                        0,
                        false,
                    ));
                }
                b"for_reply_2" => {
                    // Don't reply, message should time out
                }
                b"for_reply_3" => {
                    // Reply to the third message
                    assert_ok!(Gear::send_reply(
                        RuntimeOrigin::signed(USER_1),
                        msg.id(),
                        [3].to_vec(),
                        1_000_000_000,
                        0,
                        false,
                    ));
                }
                b"for_reply_4" => {
                    // reply later
                    timeout_msg_id = Some(msg.id());
                }
                _ => unreachable!(),
            }
        }

        run_to_block(4, None);

        // Expect a reply back
        let m = maybe_last_message(USER_1);
        assert!(m.unwrap().payload_bytes() == b"saw_reply_3");

        run_to_block(10, None);

        // Program finished
        let m = maybe_last_message(USER_1);
        assert!(m.unwrap().payload_bytes() == b"completed");

        // Reply to a message that timed out
        assert_ok!(Gear::send_reply(
            RuntimeOrigin::signed(USER_1),
            timeout_msg_id.unwrap(),
            [4].to_vec(),
            1_000_000_000,
            0,
            false,
        ));

        run_to_block(11, None);

        let messages = all_user_messages(USER_1);
        let vec: Vec<gstd::borrow::Cow<'_, str>> = messages
            .iter()
            .filter_map(|m| {
                if m.details().is_some() {
                    None
                } else {
                    Some(String::from_utf8_lossy(m.payload_bytes()))
                }
            })
            .collect();
        // Hook executed after completed
        assert_eq!(
            vec,
            [
                "for_reply_1",
                "for_reply_2",
                "for_reply_3",
                "for_reply_4",
                "saw_reply_3",
                "completed",
                "saw_reply_4"
            ]
        );
    });
}

#[test]
fn program_with_large_indexes() {
    // There is a security problem in module deserialization found by
    // casper-wasm https://github.com/casper-network/casper-wasm/pull/1,
    // parity-wasm results OOM on deserializing a module with large indexes.
    //
    // bytecodealliance/wasm-tools has similar tests:
    // https://github.com/bytecodealliance/wasm-tools/blob/main/crates/wasmparser/tests/big-module.rs
    //
    // This test is to make sure that we are not affected by the same problem.
    let code_len_limit = Limits::default().code_len;

    // Here we generate a valid program full with empty functions to reach the limit
    // of both the function indexes and the code length in our node.
    //
    // The testing program has length `60` with only 1 mocked function, each empty
    // function takes byte code size `4`.
    //
    // NOTE: Leaving 35 indexes (140 bytes) for injecting the stack limiter
    // [`wasm_instrument::InstrumentationBuilder::instrument`].
    let empty_prog_len = 60;
    let empty_fn_len = 4;
    let indexes_in_stack_limiter = 35;
    let indexes_limit = (code_len_limit - empty_prog_len) / empty_fn_len - indexes_in_stack_limiter;
    let funcs = "(func)".repeat(indexes_limit as usize);
    let wat = format!(
        r#"
          (module
           (type (func))
           (import "env" "memory" (memory (;0;) 17))
           {funcs}
           (export "handle" (func 0))
           (export "init" (func 0))
          )
    "#
    );

    let wasm = wat::parse_str(&wat).expect("failed to compile wat to wasm");
    assert!(
        code_len_limit as usize - wasm.len() < 140,
        "Failed to reach the max limit of code size."
    );

    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(&wat).to_bytes();
        assert_ok!(Gear::upload_code(RuntimeOrigin::signed(USER_1), code));
    });
}

#[test]
fn outgoing_messages_bytes_limit_exceeded() {
    let error_code = MessageError::OutgoingMessagesBytesLimitExceeded as u32;

    let wat = format!(
        r#"
        (module
            (import "env" "memory" (memory 0x100))
            (import "env" "gr_send" (func $gr_send (param i32 i32 i32 i32 i32)))
            (export "init" (func $init))
            (func $init
                (loop $loop
                    i32.const 0        ;; destination and value ptr
                    i32.const 0        ;; payload ptr
                    i32.const 0x4c0000 ;; payload length
                    i32.const 0        ;; delay
                    i32.const 0x4d0000 ;; result ptr
                    call $gr_send

                    ;; if it's not an error, then continue the loop
                    (if (i32.eqz (i32.load (i32.const 0x4d0000))) (then (br $loop)))

                    ;; if it's sought-for error, then finish successfully
                    ;; if it's unknown error, then panic
                    (if (i32.eq (i32.const {error_code}) (i32.load (i32.const 0x4d0000)))
                        (then)
                        (else unreachable)
                    )
                )
            )
            (export "__gear_stack_end" (global 0))
            (global i32 (i32.const 0x1000000))     ;; all memory
        )"#
    );

    init_logger();
    new_test_ext().execute_with(|| {
        let code = ProgramCodeKind::Custom(wat.as_str()).to_bytes();
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            code,
            DEFAULT_SALT.to_vec(),
            vec![],
            100_000_000_000,
            0,
            false,
        ));

        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_succeed(mid);
    });
}

// TODO: this test must be moved to `core-processor` crate,
// but it's not possible currently, because mock for `core-processor` does not exist #3742
#[test]
fn incorrect_store_context() {
    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Default.to_bytes(),
            DEFAULT_SALT.to_vec(),
            vec![],
            100_000_000_000,
            0,
            false,
        ));

        let pid = get_last_program_id();
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_succeed(mid);

        let gas_limit = 10_000_000_000;
        Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            vec![],
            gas_limit,
            0,
            true,
        )
        .unwrap();
        let mid = get_last_message_id();

        // Dequeue dispatch in order to queue corrupted dispatch with same id later
        QueueOf::<Test>::dequeue().unwrap().unwrap();

        // Start creating dispatch with outgoing messages total bytes limit exceeded
        let payload = Vec::new().try_into().unwrap();
        let message = IncomingMessage::new(mid, USER_1.cast(), payload, gas_limit, 0, None);

        // Get overloaded `StoreContext` using `MessageContext`
        let limit = <Test as Config>::OutgoingBytesLimit::get();
        let dispatch = IncomingDispatch::new(DispatchKind::Handle, message.clone(), None);
        let settings = ContextSettings::with_outgoing_limits(1024, limit + 1);
        let mut message_context = MessageContext::new(dispatch, pid, settings);
        let mut counter = 0;
        // Fill until the limit is reached
        while counter < limit + 1 {
            let handle = message_context.send_init().unwrap();
            let len = (Payload::MAX_LEN as u32).min(limit + 1 - counter);
            message_context
                .send_push(handle, &vec![1; len as usize])
                .unwrap();
            counter += len;
        }
        let (_, context_store) = message_context.drain();

        // Enqueue dispatch with corrupted context
        let message = message.into_stored(pid);
        let dispatch = StoredDispatch::new(DispatchKind::Handle, message, Some(context_store));
        QueueOf::<Test>::queue(dispatch).unwrap();

        run_to_next_block(None);
        // does not fail anymore, context does not keep state between executions
        assert_succeed(mid);
    });
}

#[test]
fn allocate_in_init_free_in_handle() {
    let static_pages = 16u16;
    let wat = format!(
        r#"
        (module
            (import "env" "memory" (memory {static_pages}))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (import "env" "free" (func $free (param i32) (result i32)))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $init
                (call $alloc (i32.const 1))
                drop
            )
            (func $handle
                (call $free (i32.const {static_pages}))
                drop
            )
        )
    "#
    );

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat.as_str()).to_bytes(),
            DEFAULT_SALT.to_vec(),
            vec![],
            1_000_000_000,
            0,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        let allocations = ProgramStorageOf::<Test>::allocations(program_id).unwrap_or_default();
        assert_eq!(
            allocations,
            [WasmPage::from(static_pages)].into_iter().collect()
        );

        Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            vec![],
            1_000_000_000,
            0,
            true,
        )
        .unwrap();

        run_to_next_block(None);

        let allocations = ProgramStorageOf::<Test>::allocations(program_id).unwrap_or_default();
        assert_eq!(allocations, Default::default());
    });
}

#[test]
fn create_program_with_reentrance_works() {
    use crate::{Fortitude, Preservation, fungible};
    use demo_constructor::demo_ping;
    use demo_create_program_reentrance::WASM_BINARY;
    use demo_distributor::WASM_BINARY as CHILD_WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        // Preparatory steps:
        // - upload `demo_ping` program;
        // - upload example program with the logic being tested;
        // - upload some wasm code to get a valid code id for future use.
        let (_init_mid, ping_pid) = init_constructor(demo_ping::scheme());

        run_to_next_block(None);

        assert_ok!(Gear::upload_code(
            RuntimeOrigin::signed(USER_1),
            CHILD_WASM_BINARY.to_vec(),
        ));
        let code_id = get_last_code_id();

        run_to_next_block(None);

        // Deploy reentrant program (without value first)
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            DEFAULT_SALT.to_vec(),
            ping_pid.into_origin().as_fixed_bytes().encode(),
            10_000_000_000,
            0,
            false,
        ));

        let program_id = get_last_program_id();

        run_to_next_block(None);

        // Test case 1
        // - Send message to the reentrant program with some `value` and a valid code id.
        // - Expect: the balance of the `demo_ping` program remains intact because a program
        //   creation inside the example program should have succeeded and the
        //   EXISTENTIAL_DEPOSIT has been charged for it thereby effectively having offset
        //   the available `value` amount.

        let amount = 10_000_u128;

        assert_eq!(
            <CurrencyOf<Test> as fungible::Inspect<_>>::reducible_balance(
                &ping_pid.cast(),
                Preservation::Expendable,
                Fortitude::Polite,
            ),
            0
        );

        let payload = (code_id.into_origin().as_fixed_bytes(), amount).encode();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            15_000_000_000,
            amount,
            false,
        ));

        run_to_next_block(None);

        assert_eq!(
            <CurrencyOf<Test> as fungible::Inspect<_>>::reducible_balance(
                &ping_pid.cast(),
                Preservation::Expendable,
                Fortitude::Polite,
            ),
            0
        );

        // Test case 2
        // - Send message to the reentrant program with some `value` and a non-existing code id.
        // - Expect: the balance of the `demo_ping` program is topped up by exactly `amount`
        //   owing to two things:
        //   * EXISTENTIAL_DEPOSIT is not charged when code id in `gr_create_program` is unknown;
        //   * there's a wait/wake cycle before the transfer attempt that resets the value counter.

        let payload = (crate::H256::random().as_fixed_bytes(), amount).encode();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            payload,
            20_000_000_000,
            amount,
            false,
        ));

        run_to_next_block(None);

        assert_eq!(
            <CurrencyOf<Test> as fungible::Inspect<_>>::reducible_balance(
                &ping_pid.cast(),
                Preservation::Expendable,
                Fortitude::Polite,
            ),
            amount
        );
    })
}

#[test]
fn dust_in_message_to_user_handled_ok() {
    use demo_value_sender::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        let pid = Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            vec![],
            10_000_000_000,
            1_000,
            false,
        )
        .map(|_| get_last_program_id())
        .unwrap();

        run_to_block(2, None);

        // Remove USER_1 account from the System.
        CurrencyOf::<Test>::make_free_balance_be(&USER_1, 0);

        // Test case 1: Make the program send a message to USER_1 with the value below the ED
        // and gas below the mailbox threshold.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            pid,
            (0_u64, 300_u128).encode(),
            1_000_000_000,
            0,
            false,
        ));

        run_to_block(3, None);

        // USER_1 account doesn't receive the funds; instead, the value is below ED so
        // account dies and value goes to UnusedValue
        assert_eq!(CurrencyOf::<Test>::free_balance(USER_1), 0);
        assert_eq!(pallet_gear_bank::UnusedValue::<Test>::get(), 300);

        // Test case 2: Make the program send a message to USER_1 with the value below the ED
        // and gas sufficient for a message to be placed into the mailbox (for 30 blocks).
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_2),
            pid,
            (3_000_u64, 300_u128).encode(),
            1_000_000_000,
            0,
            false,
        ));

        run_to_block(40, None);

        // USER_1 account doesn't receive the funds again; instead, the value is stored as the
        // `UnusedValue` in Gear Bank.
        assert_eq!(CurrencyOf::<Test>::free_balance(USER_1), 0);
        assert_eq!(pallet_gear_bank::UnusedValue::<Test>::get(), 600);
    });
}

#[test]
fn test_gasless_steal_gas_for_wait() {
    init_logger();
    new_test_ext().execute_with(|| {
        use demo_constructor::{Arg, Calls, Scheme};

        let wait_duration = 10;
        let handle = Calls::builder()
            .source("source_store")
            .send("source_store", Arg::new("msg1".encode()))
            .send("source_store", Arg::new("msg2".encode()))
            .send("source_store", Arg::new("msg3".encode()))
            .send("source_store", Arg::new("msg4".encode()))
            .send("source_store", Arg::new("msg5".encode()))
            .wait_for(wait_duration);
        let scheme = Scheme::predefined(
            Calls::builder().noop(),
            handle,
            Calls::builder().noop(),
            Calls::builder().noop(),
        );

        let (_, pid) = init_constructor(scheme);
        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            USER_1.into_origin(),
            HandleKind::Handle(pid),
            Default::default(),
            0,
            true,
            true,
        )
        .expect("calculate_gas_info failed");
        let waiting_bound = HoldBoundBuilder::<Test>::new(StorageType::Waitlist)
            .duration(wait_duration.unique_saturated_into());

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            pid,
            Default::default(),
            min_limit - waiting_bound.lock_amount(),
            0,
            true
        ));
        let mid = get_last_message_id();

        run_to_next_block(None);

        assert_failed(
            mid,
            ErrorReplyReason::Execution(SimpleExecutionError::BackendError),
        );
        assert_eq!(MailboxOf::<Test>::len(&USER_1), 0);
    })
}

#[test]
fn use_big_memory() {
    let last_4_bytes_offset = WasmPage::from(MAX_WASM_PAGES_AMOUNT).offset() - 4;
    let middle_4_bytes_offset = WasmPage::from(MAX_WASM_PAGES_AMOUNT / 2).offset();
    let last_page_number = MAX_WASM_PAGES_AMOUNT.checked_sub(1).unwrap();

    let wat = format!(
        r#"
        (module
		    (import "env" "memory" (memory 0))
            (import "env" "alloc" (func $alloc (param i32) (result i32)))
            (import "env" "free_range" (func $free_range (param i32) (param i32) (result i32)))
            (export "init" (func $init))
            (export "handle" (func $handle))
            (func $init
                (drop (call $alloc (i32.const {MAX_WASM_PAGES_AMOUNT})))

                ;; access last 4 bytes
                (i32.store (i32.const {last_4_bytes_offset}) (i32.const 0x42))

                ;; access first 4 bytes
                (i32.store (i32.const 0) (i32.const 0x42))

                ;; access 4 bytes in the middle
                (i32.store (i32.const {middle_4_bytes_offset}) (i32.const 0x42))
            )
            (func $handle
                (drop (call $free_range (i32.const 0) (i32.const {last_page_number})))
            )
        )"#
    );

    init_logger();
    new_test_ext().execute_with(|| {
        Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            ProgramCodeKind::Custom(wat.as_str()).to_bytes(),
            DEFAULT_SALT.to_vec(),
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            true,
        )
        .unwrap();

        let program_id = get_last_program_id();

        run_to_next_block(None);
        assert_last_dequeued(1);

        let expected_allocations: IntervalsTree<WasmPage> =
            [numerated::interval::Interval::try_from(
                WasmPage::from(0)..WasmPage::from(MAX_WASM_PAGES_AMOUNT),
            )
            .unwrap()]
            .into_iter()
            .collect();

        assert_eq!(
            ProgramStorageOf::<Test>::allocations(program_id),
            Some(expected_allocations),
        );

        let program = ProgramStorageOf::<Test>::get_program(program_id).expect("Program not found");
        let Program::Active(program) = program else {
            panic!("Program is not active");
        };

        assert_eq!(program.allocations_tree_len, 1);

        let pages_with_data =
            <ProgramStorageOf<Test> as ProgramStorage>::MemoryPageMap::iter_prefix(
                &program_id,
                &program.memory_infix,
            )
            .map(|(page, buf)| {
                assert_eq!(buf.iter().copied().sum::<u8>(), 0x42);
                page
            })
            .collect::<Vec<_>>();

        assert_eq!(
            pages_with_data,
            vec![
                GearPage::from_offset(0),
                GearPage::from_offset(middle_4_bytes_offset),
                GearPage::from_offset(last_4_bytes_offset)
            ]
        );

        Gear::send_message(
            RuntimeOrigin::signed(USER_1),
            program_id,
            EMPTY_PAYLOAD.to_vec(),
            10_000_000_000,
            0,
            true,
        )
        .unwrap();

        run_to_next_block(None);
        assert_last_dequeued(1);

        assert_eq!(
            ProgramStorageOf::<Test>::allocations(program_id),
            Some(Default::default()),
        );

        assert_eq!(
            <ProgramStorageOf<Test> as ProgramStorage>::MemoryPageMap::iter_prefix(
                &program_id,
                &program.memory_infix,
            )
            .count(),
            0
        );
    });
}

#[test]
fn vec() {
    use demo_vec::WASM_BINARY;

    init_logger();
    new_test_ext().execute_with(|| {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(1),
            WASM_BINARY.to_vec(),
            b"salt".to_vec(),
            vec![],
            10_000_000_000,
            0,
            false,
        ));

        let vec_id = get_last_program_id();

        run_to_next_block(None);

        let code_id = CodeId::generate(WASM_BINARY);

        let code_metadata = <Test as Config>::CodeStorage::get_code_metadata(code_id)
            .expect("code should be in the storage");

        let static_pages = code_metadata.static_pages();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(1),
            vec_id,
            131072i32.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block(None);

        let reply = maybe_last_message(1).expect("Should be");
        assert_eq!(reply.payload_bytes(), 131072i32.encode());

        assert!(QueueOf::<Test>::is_empty());

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(vec_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        assert_eq!(program.code_id, code_id);

        let pages = ProgramStorageOf::<Test>::get_program_pages_data(vec_id, program.memory_infix)
            .expect("Program pages data not found")
            .keys()
            .fold(BTreeSet::new(), |mut set, page| {
                let wasm_page: WasmPage = page.to_page();
                if wasm_page >= static_pages {
                    set.insert(u32::from(wasm_page));
                }
                set
            });

        let pages = pages.into_iter().collect::<Vec<_>>();
        assert_eq!(pages, vec![17, 18]);
    });
}

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
        let program_id = ActorId::generate_from_user(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        assert_ok!(Gear::upload_program(
            origin.clone(),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
            false,
        ));

        run_to_block(2, None);

        let gear_page0 = GearPage::from_offset(0x0);
        let mut page0_data = PageBuf::new_zeroed();
        page0_data[0] = 0x42;

        let gear_page7 = GearPage::from_offset(0x70000);
        let mut page7_data = PageBuf::new_zeroed();
        page7_data[0] = 0x42;

        let mut persistent_pages = BTreeMap::new();
        persistent_pages.insert(gear_page0, page0_data.clone());
        persistent_pages.insert(gear_page7, page7_data);

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        let program_static_pages =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id)
                .expect("Failed to get code metadata")
                .static_pages();

        let program_persistent_pages =
            ProgramStorageOf::<Test>::get_program_pages_data(program_id, program.memory_infix)
                .expect("Failed to get program pages data");

        let expected_static_pages = WasmPage::from(0);

        assert_eq!(program_static_pages, expected_static_pages);
        assert_eq!(program_persistent_pages, persistent_pages);

        assert_ok!(Gear::send_message(
            origin,
            program_id,
            vec![],
            5_000_000_000_u64,
            0_u128,
            false,
        ));

        run_to_block(3, None);

        page0_data[0] = 0x1;
        persistent_pages.insert(gear_page0, page0_data);

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        let program_static_pages =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id)
                .expect("Failed to get code metadata")
                .static_pages();

        let program_persistent_pages =
            ProgramStorageOf::<Test>::get_program_pages_data(program_id, program.memory_infix)
                .expect("Failed to get program pages data");

        let expected_static_pages = WasmPage::from(0);

        assert_eq!(program_static_pages, expected_static_pages);
        assert_eq!(program_persistent_pages, persistent_pages);
    })
}

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
        let program_id = ActorId::generate_from_user(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        // Code info. Must be in consensus with wasm code.
        let static_pages: WasmPagesAmount = 8.into();
        let page1_accessed_addr = 0x10000;
        let page3_accessed_addr = 0x3fffd;
        let page4_accessed_addr = 0x40000;
        let page8_accessed_addr = 0x87654;
        let page9_accessed_addr = 0x98765;

        assert_ok!(Gear::upload_program(
            origin.clone(),
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
            false,
        ));

        run_to_block(2, None);

        let mut persistent_pages = BTreeMap::new();

        let gear_page1 = GearPage::from_offset(page1_accessed_addr);
        let mut page1_data = PageBuf::new_zeroed();
        page1_data[..10].copy_from_slice(b"0123456780".as_slice());

        let gear_page8 = GearPage::from_offset(page8_accessed_addr);
        let mut page8_data = PageBuf::new_zeroed();
        page8_data[(page8_accessed_addr % GearPage::SIZE) as usize] = 0x42;

        let gear_page9 = GearPage::from_offset(page9_accessed_addr);
        let mut page9_data = PageBuf::new_zeroed();
        page9_data[(page9_accessed_addr % GearPage::SIZE) as usize] = 0x42;

        persistent_pages.insert(gear_page1, page1_data);
        persistent_pages.insert(gear_page8, page8_data);
        persistent_pages.insert(gear_page9, page9_data);

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        let program_static_pages =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id)
                .expect("Failed to get code metadata")
                .static_pages();

        let program_persistent_pages =
            ProgramStorageOf::<Test>::get_program_pages_data(program_id, program.memory_infix)
                .expect("Failed to get program pages data");

        assert_eq!(program_static_pages, static_pages);
        assert_eq!(program_persistent_pages, persistent_pages);

        assert_ok!(Gear::send_message(
            origin,
            program_id,
            vec![],
            5_000_000_000_u64,
            0_u128,
            false,
        ));

        run_to_block(3, None);

        let gear_page3 = GearPage::from_offset(page3_accessed_addr);
        let mut page3_data = PageBuf::new_zeroed();
        page3_data[(page3_accessed_addr % GearPage::SIZE) as usize] = 0x42;

        let gear_page4 = GearPage::from_offset(page4_accessed_addr);

        persistent_pages.insert(gear_page3, page3_data);
        persistent_pages.insert(gear_page4, PageBuf::new_zeroed());

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        let program_static_pages =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id)
                .expect("Failed to get code metadata")
                .static_pages();

        let program_persistent_pages =
            ProgramStorageOf::<Test>::get_program_pages_data(program_id, program.memory_infix)
                .expect("Failed to get program pages data");

        assert_eq!(program_static_pages, static_pages);
        assert_eq!(program_persistent_pages, persistent_pages);
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
        let code = utils::parse_wat(wat.as_str());
        let program_id = ActorId::generate_from_user(CodeId::generate(&code), DEFAULT_SALT);
        let origin = RuntimeOrigin::signed(1);

        assert_ok!(Gear::upload_program(
            origin,
            code.clone(),
            DEFAULT_SALT.to_vec(),
            Vec::new(),
            5_000_000_000_u64,
            0_u128,
            false,
        ));

        run_to_block(2, None);

        let mut persistent_pages = BTreeMap::new();

        let gear_page2 = WasmPage::from(2).to_page();
        let gear_page3 = WasmPage::from(3).to_page();
        let mut page_data = PageBuf::new_zeroed();
        page_data[0] = 0x42;

        persistent_pages.insert(gear_page2, page_data.clone());
        persistent_pages.insert(gear_page3, page_data);

        let program: ActiveProgram<_> = ProgramStorageOf::<Test>::get_program(program_id)
            .expect("Failed to find program with such id")
            .try_into()
            .expect("Program should be active");

        let program_static_pages =
            <Test as Config>::CodeStorage::get_code_metadata(program.code_id)
                .expect("Failed to get code metadata")
                .static_pages();

        let program_persistent_pages =
            ProgramStorageOf::<Test>::get_program_pages_data(program_id, program.memory_infix)
                .expect("Failed to get program pages data");

        let expected_static_pages = WasmPagesAmount::from(4);

        assert_eq!(program_static_pages, expected_static_pages);
        assert_eq!(program_persistent_pages, persistent_pages);
    })
}

pub(crate) mod utils {
    #![allow(unused)]

    use super::{
        BlockNumber, Event, MailboxOf, MockRuntimeEvent, RuntimeOrigin, Test, assert_ok, pallet,
        run_to_block,
    };
    use crate::{
        BalanceOf, BlockGasLimitOf, BuiltinDispatcherFactory, Config, CurrencyOf,
        EXISTENTIAL_DEPOSIT_LOCK_ID, GasHandlerOf, GasInfo, GearBank, HandleKind, ProgramStorageOf,
        QueueOf, SentOf,
        mock::{Balances, Gear, System, USER_1, run_to_next_block},
    };
    use common::{
        CodeStorage, Origin, ProgramStorage, ReservableTree,
        event::*,
        storage::{CountedByKey, Counter, IterableByKeyMap, IterableMap},
    };
    use core::{fmt, fmt::Display};
    use core_processor::common::ActorExecutionErrorReplyReason;
    use demo_constructor::{Scheme, WASM_BINARY as DEMO_CONSTRUCTOR_WASM_BINARY};
    use frame_support::{
        dispatch::{DispatchErrorWithPostInfo, DispatchResultWithPostInfo},
        traits::tokens::{Balance, currency::Currency},
    };
    use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
    use gear_core::{
        buffer::Payload,
        ids::{ActorId, CodeId, MessageId, prelude::*},
        memory::PageBuf,
        message::{Message, ReplyDetails, StoredDispatch, UserMessage, UserStoredMessage},
        pages::{GearPage, WasmPagesAmount},
        program::{ActiveProgram, Program},
        reservation::GasReservationMap,
    };
    use gear_core_errors::*;
    use gstd::TypeInfo;
    use pallet_gear_voucher::VoucherId;
    use parity_scale_codec::Encode;
    use sp_core::H256;
    use sp_runtime::{codec::Decode, traits::UniqueSaturatedInto};
    use sp_std::{convert::TryFrom, fmt::Debug};
    use std::{
        collections::{BTreeMap, BTreeSet},
        iter,
    };

    pub(super) const DEFAULT_GAS_LIMIT: u64 = 200_000_000;
    pub(super) const DEFAULT_SALT: &[u8; 4] = b"salt";
    pub(super) const EMPTY_PAYLOAD: &[u8; 0] = b"";
    pub(super) const OUTGOING_WITH_VALUE_IN_HANDLE_VALUE_GAS: u64 = 10000000;
    // This program just waits on each handle message.
    pub(super) const WAITER_WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_wait" (func $gr_wait))
            (export "handle" (func $handle))
            (func $handle call $gr_wait)
        )"#;

    pub(super) type DispatchCustomResult<T> = Result<T, DispatchErrorWithPostInfo>;
    pub(super) type AccountId = <Test as frame_system::Config>::AccountId;

    pub(super) fn hash(data: impl AsRef<[u8]>) -> [u8; 32] {
        sp_core::blake2_256(data.as_ref())
    }

    pub fn init_logger() {
        let _ = tracing_subscriber::fmt::try_init();
    }

    #[track_caller]
    pub(crate) fn submit_constructor_with_args(
        origin: AccountId,
        salt: impl AsRef<[u8]>,
        scheme: Scheme,
        value: BalanceOf<Test>,
    ) -> (MessageId, ActorId) {
        let GasInfo { min_limit, .. } = Gear::calculate_gas_info(
            origin.into_origin(),
            HandleKind::Init(DEMO_CONSTRUCTOR_WASM_BINARY.to_vec()),
            scheme.encode(),
            value,
            true,
            true,
        )
        .expect("calculate_gas_info failed");

        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(origin),
            DEMO_CONSTRUCTOR_WASM_BINARY.to_vec(),
            salt.as_ref().to_vec(),
            scheme.encode(),
            min_limit,
            value,
            false,
        ));

        (get_last_message_id(), get_last_program_id())
    }

    #[track_caller]
    pub(crate) fn init_constructor_with_value(
        scheme: Scheme,
        value: BalanceOf<Test>,
    ) -> (MessageId, ActorId) {
        let res = submit_constructor_with_args(USER_1, DEFAULT_SALT, scheme, value);

        run_to_next_block(None);
        assert!(is_active(res.1));

        res
    }

    pub(crate) fn is_active(program_id: ActorId) -> bool {
        let (builtins, _) = <Test as crate::Config>::BuiltinDispatcherFactory::create();

        Gear::is_active(&builtins, program_id)
    }

    #[track_caller]
    pub(crate) fn init_constructor(scheme: Scheme) -> (MessageId, ActorId) {
        init_constructor_with_value(scheme, 0)
    }

    #[track_caller]
    pub(super) fn assert_balance(
        origin: impl common::Origin,
        free: impl Into<BalanceOf<Test>>,
        reserved: impl Into<BalanceOf<Test>>,
    ) {
        let account_id = origin.cast();
        assert_eq!(
            Balances::free_balance(account_id),
            free.into(),
            "Free balance"
        );
        assert_eq!(
            GearBank::<Test>::account_total(&account_id),
            reserved.into(),
            "Reserved balance"
        );
    }

    #[track_caller]
    pub(super) fn assert_program_balance<B>(
        origin: impl common::Origin,
        available: B,
        locked: B,
        reserved: B,
    ) where
        B: Into<BalanceOf<Test>> + Copy,
    {
        let account_id: u64 = origin.cast();
        let available = available.into();
        let locked = locked.into();
        let reserved = reserved.into();

        let account_data = System::account(account_id).data;
        assert_eq!(
            account_data.free,
            available + locked,
            "Free balance of {available} + {locked} (available + locked)"
        );
        assert_eq!(account_data.frozen, locked, "Frozen balance");
        let maybe_ed = Balances::locks(&account_id)
            .into_iter()
            .filter_map(|lock| {
                if lock.id == EXISTENTIAL_DEPOSIT_LOCK_ID {
                    Some(lock.amount)
                } else {
                    None
                }
            })
            .reduce(|a, b| a + b)
            .unwrap_or_default();
        assert_eq!(maybe_ed, locked, "Locked ED");
        assert_eq!(
            GearBank::<Test>::account_total(&account_id),
            reserved,
            "Reserved balance"
        );
    }

    #[track_caller]
    pub(super) fn calculate_handle_and_send_with_extra(
        origin: AccountId,
        destination: ActorId,
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
            true,
        )
        .expect("calculate_gas_info failed");

        let limit = gas_info.min_limit + gas_limit.unwrap_or_default();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(origin),
            destination,
            payload,
            limit,
            value,
            false,
        ));

        let message_id = get_last_message_id();

        (message_id, gas_info)
    }

    pub(super) fn get_ed() -> u128 {
        CurrencyOf::<Test>::minimum_balance().unique_saturated_into()
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
    pub(crate) fn assert_last_dequeued(expected: u32) {
        let last_dequeued = System::events()
            .iter()
            .filter_map(|e| {
                if let MockRuntimeEvent::Gear(Event::MessagesDispatched { total, .. }) = e.event {
                    Some(total)
                } else {
                    None
                }
            })
            .next_back()
            .expect("Not found RuntimeEvent::MessagesDispatched");

        assert_eq!(expected, last_dequeued);
    }

    #[track_caller]
    pub(super) fn assert_total_dequeued(expected: u32) {
        System::events().iter().for_each(|e| {
            log::debug!("Event: {:?}", e);
        });

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
        prog_id: ActorId,
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
            false,
        ));

        let message_id = get_last_message_id();
        run_to_block(block_num, None);

        {
            let expected_code = ProgramCodeKind::OutgoingWithValueInHandle.to_bytes();
            assert_eq!(
                ProgramStorageOf::<Test>::get_program(prog_id)
                    .and_then(|program| ActiveProgram::try_from(program).ok())
                    .expect("program must exist")
                    .code_id,
                generate_code_hash(&expected_code).into(),
                "can invoke send to mailbox only from `ProgramCodeKind::OutgoingWithValueInHandle` program"
            );
        }

        MessageId::generate_outgoing(message_id, 0)
    }

    #[track_caller]
    pub(super) fn increase_prog_balance_for_mailbox_test(sender: AccountId, program_id: ActorId) {
        let expected_code_hash: CodeId = generate_code_hash(
            ProgramCodeKind::OutgoingWithValueInHandle
                .to_bytes()
                .as_slice(),
        )
        .into();
        let actual_code_hash = ProgramStorageOf::<Test>::get_program(program_id)
            .and_then(|program| ActiveProgram::try_from(program).ok())
            .map(|prog| prog.code_id)
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
            &program_id.cast(),
            locked_value,
            frame_support::traits::ExistenceRequirement::AllowDeath
        ));
    }

    // Submits program with default options (salt, gas limit, value, payload)
    #[track_caller]
    pub(super) fn upload_program_default(
        user: AccountId,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<ActorId> {
        upload_program_default_with_salt(user, DEFAULT_SALT.to_vec(), code_kind)
    }

    // Submits program with default options (gas limit, value, payload)
    #[track_caller]
    pub(super) fn upload_program_default_with_salt(
        user: AccountId,
        salt: Vec<u8>,
        code_kind: ProgramCodeKind,
    ) -> DispatchCustomResult<ActorId> {
        let code = code_kind.to_bytes();

        Gear::upload_program(
            RuntimeOrigin::signed(user),
            code,
            salt,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
            false,
        )
        .map(|_| get_last_program_id())
    }

    pub(super) fn generate_program_id(code: &[u8], salt: &[u8]) -> ActorId {
        ActorId::generate_from_user(CodeId::generate(code), salt)
    }

    pub(super) fn generate_code_hash(code: &[u8]) -> [u8; 32] {
        CodeId::generate(code).into()
    }

    pub(super) fn send_default_message(from: AccountId, to: ActorId) -> DispatchResultWithPostInfo {
        Gear::send_message(
            RuntimeOrigin::signed(from),
            to,
            EMPTY_PAYLOAD.to_vec(),
            DEFAULT_GAS_LIMIT,
            0,
            false,
        )
    }

    pub(super) fn call_default_message(to: ActorId) -> crate::mock::RuntimeCall {
        crate::mock::RuntimeCall::Gear(crate::Call::<Test>::send_message {
            destination: to,
            payload: EMPTY_PAYLOAD.to_vec(),
            gas_limit: DEFAULT_GAS_LIMIT,
            value: 0,
            keep_alive: false,
        })
    }

    #[track_caller]
    pub(super) fn dispatch_status(message_id: MessageId) -> Option<DispatchStatus> {
        let mut found_status: Option<DispatchStatus> = None;
        System::events().iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::MessagesDispatched { statuses, .. }) = &e.event {
                found_status = statuses.get(&message_id).cloned();
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

    #[track_caller]
    fn get_last_event_error_and_reply_code(message_id: MessageId) -> (Vec<u8>, ReplyCode) {
        let mut actual_error = None;

        System::events().into_iter().for_each(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event
                && let Some(details) = message.details()
            {
                let (mid, code) = details.into_parts();
                if mid == message_id && code.is_error() {
                    actual_error = Some((message.payload_bytes().to_vec(), code));
                }
            }
        });

        let (actual_error, reply_code) =
            actual_error.expect("Error message not found in any `RuntimeEvent::UserMessageSent`");

        log::debug!(
            "Actual error: '{}'\nReply code: {reply_code:?}",
            std::str::from_utf8(&actual_error).unwrap_or("<bytes>")
        );

        (actual_error, reply_code)
    }

    #[derive(derive_more::Display, derive_more::From)]
    pub(super) enum AssertFailedError {
        Panic(String),
        SimpleReply(ErrorReplyReason),
    }

    #[track_caller]
    pub(super) fn assert_failed(message_id: MessageId, error: impl Into<AssertFailedError>) {
        let error = error.into();
        let status =
            dispatch_status(message_id).expect("Message not found in `Event::MessagesDispatched`");

        assert_eq!(status, DispatchStatus::Failed, "Expected: {error}");

        let (mut actual_error, reply_code) = get_last_event_error_and_reply_code(message_id);

        if let ReplyCode::Error(err_reason) = reply_code {
            if err_reason.is_exited() {
                // ActorId.
                assert_eq!(actual_error.len(), 32);
            } else if !err_reason.is_userspace_panic() {
                assert!(actual_error.is_empty());
            }
        }

        match error {
            AssertFailedError::Panic(error) => {
                let mut expectations = error.to_string();
                let mut actual_error = String::from_utf8(actual_error).unwrap();

                // In many cases fallible syscall returns ExtError, which program unwraps afterwards.
                // This check handles display of the error inside.
                if actual_error.starts_with('\'') {
                    let j = actual_error.rfind('\'').expect("Checked above");
                    actual_error = String::from(&actual_error[..(j + 1)]);
                    expectations = format!("'{expectations}'");
                }

                assert_eq!(expectations, actual_error);
            }
            AssertFailedError::SimpleReply(error) => {
                assert_eq!(reply_code, ReplyCode::error(error));
            }
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
            .next_back()
            .expect("failed to get last event")
            .event
    }

    #[track_caller]
    pub(super) fn get_last_program_id() -> ActorId {
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
    pub(super) fn get_last_voucher_id() -> VoucherId {
        System::events()
            .iter()
            .rev()
            .filter_map(|r| {
                if let MockRuntimeEvent::GearVoucher(e) = r.event.clone() {
                    Some(e)
                } else {
                    None
                }
            })
            .find_map(|e| match e {
                pallet_gear_voucher::Event::VoucherIssued { voucher_id, .. } => Some(voucher_id),
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
    pub(super) fn get_mailbox_expiration(message_id: MessageId) -> BlockNumberFor<Test> {
        let mut exp = None;
        System::events()
            .into_iter()
            .rfind(|e| match &e.event {
                MockRuntimeEvent::Gear(Event::UserMessageSent {
                    message,
                    expiration: Some(expiration),
                    ..
                }) => {
                    if message.id() == message_id {
                        exp = Some(*expiration);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            })
            .expect("Failed to find appropriate UserMessageSent event");

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
    pub(super) fn maybe_last_message(account: impl Origin) -> Option<UserMessage> {
        let account = account.cast();

        System::events().into_iter().rev().find_map(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                if message.destination() == account {
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
    // returns (amount of messages sent, amount of messages sent **to mailbox**)
    pub(super) fn user_messages_sent() -> (usize, usize) {
        System::events()
            .into_iter()
            .fold((0usize, 0usize), |(total, to_mailbox), e| {
                if let MockRuntimeEvent::Gear(Event::UserMessageSent { expiration, .. }) = e.event {
                    (total + 1, to_mailbox + expiration.is_some() as usize)
                } else {
                    (total, to_mailbox)
                }
            })
    }

    #[track_caller]
    pub(super) fn maybe_any_last_message() -> Option<UserMessage> {
        System::events().into_iter().rev().find_map(|e| {
            if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                Some(message)
            } else {
                None
            }
        })
    }

    #[track_caller]
    pub(super) fn get_last_mail(account: impl Origin) -> UserStoredMessage {
        MailboxOf::<Test>::iter_key(account.cast())
            .last()
            .map(|(msg, _bn)| msg)
            .expect("Element should be")
    }

    #[track_caller]
    pub(super) fn get_reservation_map(pid: ActorId) -> Option<GasReservationMap> {
        let program = ProgramStorageOf::<Test>::get_program(pid).unwrap();
        if let Program::Active(ActiveProgram {
            gas_reservation_map,
            ..
        }) = program
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

    impl ProgramCodeKind<'_> {
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
                    // Handle function must exist, while init must not for auto-replies tests.
                    //
                    // Sending message to USER_1 is hardcoded!
                    // Program sends message in handle which sets gas limit to 10_000_000 and value to 1000.
                    // [warning] - program payload data is inaccurate, don't make assumptions about it!
                    r#"
                    (module
                        (import "env" "gr_send_wgas" (func $send (param i32 i32 i32 i64 i32 i32)))
                        (import "env" "memory" (memory 1))
                        (export "handle" (func $handle))
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
                    )"#
                }
                ProgramCodeKind::Custom(code) => code,
                ProgramCodeKind::CustomInvalid(code) => {
                    validate = false;
                    code
                }
            };

            let code = wat::parse_str(source).expect("failed to parse module");
            if validate {
                wasmparser::validate(&code).expect("failed to validate module");
            }
            code
        }
    }

    pub(crate) fn print_gear_events() {
        let v = System::events()
            .into_iter()
            .map(|r| r.event)
            .collect::<Vec<_>>();

        println!("Gear events");
        for (pos, line) in v.iter().enumerate() {
            println!("{pos}). {line:?}");
        }
    }

    #[track_caller]
    pub(super) fn send_payloads(
        user_id: AccountId,
        program_id: ActorId,
        payloads: Vec<Vec<u8>>,
    ) -> Vec<MessageId> {
        payloads
            .into_iter()
            .map(|payload| {
                assert_ok!(Gear::send_message(
                    RuntimeOrigin::signed(user_id),
                    program_id,
                    payload,
                    BlockGasLimitOf::<Test>::get(),
                    0,
                    false,
                ));

                get_last_message_id()
            })
            .collect()
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) enum Assertion {
        Payload(Vec<u8>),
        ReplyCode(ReplyCode),
    }

    #[track_caller]
    pub(crate) fn assert_responses_to_user(user_id: impl Origin, assertions: Vec<Assertion>) {
        let user_id = user_id.cast();

        let messages: Vec<UserMessage> = System::events()
            .into_iter()
            .filter_map(|e| {
                if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                    Some(message)
                } else {
                    None
                }
            })
            .filter(|message| message.destination() == user_id)
            .collect();

        if messages.len() != assertions.len() {
            panic!(
                "Expected {} messages, you assert only {} of them\n{:#?}",
                messages.len(),
                assertions.len(),
                messages
            )
        }

        let res: Vec<Assertion> = iter::zip(messages, &assertions)
            .map(|(message, assertion)| {
                match assertion {
                    Assertion::Payload(_) => Assertion::Payload(message.payload_bytes().to_vec()),
                    Assertion::ReplyCode(_) => {
                        // `ReplyCode::Unsupported` used to avoid options here.
                        Assertion::ReplyCode(message.reply_code().unwrap_or(ReplyCode::Unsupported))
                    }
                }
            })
            .collect();

        assert_eq!(res, assertions);
    }

    #[track_caller]
    pub(super) fn test_signal_code_works(
        signal_code: SignalCode,
        action: demo_signal_entry::HandleAction,
    ) {
        use crate::tests::new_test_ext;
        use demo_signal_entry::{HandleAction, WASM_BINARY};

        const GAS_LIMIT: u64 = 13_000_000_000;

        init_logger();
        new_test_ext().execute_with(|| {
            // Upload program
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(USER_1),
                WASM_BINARY.to_vec(),
                DEFAULT_SALT.to_vec(),
                USER_1.encode(),
                GAS_LIMIT,
                0,
                false,
            ));

            let pid = get_last_program_id();

            run_to_next_block(None);

            // Ensure that program is uploaded and initialized correctly
            assert!(is_active(pid));
            assert!(Gear::is_initialized(pid));

            // Save signal code to be compared with
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                HandleAction::SaveSignal(signal_code).encode(),
                GAS_LIMIT,
                0,
                false,
            ));

            run_to_next_block(None);

            // Send the action to trigger signal sending
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(USER_1),
                pid,
                action.encode(),
                GAS_LIMIT,
                0,
                false,
            ));

            let mid = get_last_message_id();

            // Assert that system reserve gas node is removed
            assert_ok!(GasHandlerOf::<Test>::get_system_reserve(mid));

            run_to_next_block(None);

            assert!(GasHandlerOf::<Test>::get_system_reserve(mid).is_err());

            // Ensure that signal code sent is signal code we saved
            let mail_msg = get_last_mail(USER_1);
            assert_eq!(mail_msg.payload_bytes(), true.encode());
        });
    }

    pub(super) fn gas_price(gas: u64) -> u128 {
        <Test as pallet_gear_bank::Config>::GasMultiplier::get().gas_to_value(gas)
    }

    // Collect all messages by account in chronological order (oldest first)
    #[track_caller]
    pub(super) fn all_user_messages(user_id: impl Origin) -> Vec<UserMessage> {
        let user_id = user_id.cast();

        System::events()
            .into_iter()
            .filter_map(|e| {
                if let MockRuntimeEvent::Gear(Event::UserMessageSent { message, .. }) = e.event {
                    if message.destination() == user_id {
                        Some(message)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    pub(super) fn parse_wat(source: &str) -> Vec<u8> {
        let code = wat::parse_str(source).expect("failed to parse module");
        wasmparser::validate(&code).expect("failed to validate module");
        code
    }

    pub(super) fn h256_code_hash(code: &[u8]) -> H256 {
        CodeId::generate(code).into_origin()
    }
}
