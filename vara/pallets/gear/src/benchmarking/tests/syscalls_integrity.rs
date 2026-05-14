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

//! Testing integration level of syscalls
//!
//! Integration level is the level between the user (`gcore`/`gstd`) and `core-backend`.
//! Tests here does not check complex business logic, but only the fact that all the
//! requested data is received properly, i.e., pointers receive expected types, no export func
//! signature map errors.
//!
//! `gr_read` is tested in the `test_syscall` program by calling `msg::load` to decode each syscall type.
//! `gr_exit` and `gr_wait*` call are not intended to be tested with the integration level tests, but only
//! with business logic tests in the separate module.

use super::*;

use crate::{BlockGasLimitOf, CurrencyOf, Event, String, WaitlistOf};
use common::event::DispatchStatus;
use frame_support::traits::Randomness;
use gear_core::ids::{CodeId, ReservationId, prelude::*};
use gear_core_errors::{ReplyCode, SuccessReplyReason};
use gear_wasm_instrument::{BlockType, Function, Instruction, MemArg, syscalls::SyscallName};
use pallet_timestamp::Pallet as TimestampPallet;
use parity_scale_codec::{Decode, Encode, FullCodec};
use test_syscalls::{Kind, WASM_BINARY as SYSCALLS_TEST_WASM_BINARY};

pub fn read_big_state<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_read_big_state::{State, Strings, WASM_BINARY};

    #[cfg(feature = "std")]
    utils::init_logger();

    let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
    let _ = CurrencyOf::<T>::deposit_creating(
        &origin,
        100_000_000_000_000_000_u128.unique_saturated_into(),
    );

    let salt = b"read_big_state salt";

    Gear::<T>::upload_program(
        RawOrigin::Signed(origin.clone()).into(),
        WASM_BINARY.to_vec(),
        salt.to_vec(),
        Default::default(),
        BlockGasLimitOf::<T>::get(),
        Zero::zero(),
        false,
    )
    .expect("Failed to upload read_big_state binary");

    let pid = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), salt);
    utils::run_to_next_block::<T>(None);

    let string = String::from("hi").repeat(4095);
    let string_size = 8 * 1024;
    assert_eq!(string.encoded_size(), string_size);

    let strings = Strings::new(string);
    let strings_size = (string_size * Strings::LEN) + 1;
    assert_eq!(strings.encoded_size(), strings_size);

    let approx_size =
        |size: usize, iteration: usize| -> usize { size - 17 - 144 * (iteration + 1) };

    // with initial data step is ~2 MiB
    let expected_size =
        |iteration: usize| -> usize { Strings::LEN * State::LEN * string_size * (iteration + 1) };

    // go to 6 MiB due to approximate calculations and 8MiB reply restrictions
    for i in 0..3 {
        let next_user_mid = utils::get_next_message_id::<T>(origin.clone());

        Gear::<T>::send_message(
            RawOrigin::Signed(origin.clone()).into(),
            pid,
            strings.encode(),
            BlockGasLimitOf::<T>::get(),
            Zero::zero(),
            false,
        )
        .expect("Failed to send read_big_state append command");

        utils::run_to_next_block::<T>(None);

        assert!(
            SystemPallet::<T>::events().into_iter().any(|e| {
                let bytes = e.event.encode();
                let Ok(gear_event): Result<Event<T>, _> = Event::decode(&mut bytes[1..].as_ref())
                else {
                    return false;
                };
                let Event::MessagesDispatched { statuses, .. } = gear_event else {
                    return false;
                };

                log::debug!("{statuses:?}");
                log::debug!("{next_user_mid:?}");
                matches!(statuses.get(&next_user_mid), Some(DispatchStatus::Success))
            }),
            "No message with expected id had succeeded"
        );

        let state = Gear::<T>::read_state_impl(pid, Default::default(), None)
            .expect("Failed to read state");
        assert_eq!(approx_size(state.len(), i), expected_size(i));
    }
}

/// We can't use `test_signal_code_works` from pallet tests because
/// this test runs on the wasmi executor and not the wasmtime.
///
/// So we just copy the code from this test and put it into the pallet benchmarks.
pub fn signal_stack_limit_exceeded_works<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_signal_entry::{HandleAction, WASM_BINARY};
    use frame_support::assert_ok;
    use gear_core_errors::*;

    const GAS_LIMIT: u64 = 10_000_000_000;

    #[cfg(feature = "std")]
    utils::init_logger();

    let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
    let _ = CurrencyOf::<T>::deposit_creating(
        &origin,
        5_000_000_000_000_000_u128.unique_saturated_into(),
    );

    let salt = b"signal_stack_limit_exceeded_works salt";

    // Upload program
    assert_ok!(Gear::<T>::upload_program(
        RawOrigin::Signed(origin.clone()).into(),
        WASM_BINARY.to_vec(),
        salt.to_vec(),
        origin.encode(),
        GAS_LIMIT,
        Zero::zero(),
        false,
    ));

    let pid = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), salt);
    utils::run_to_next_block::<T>(None);

    // Ensure that program is uploaded and initialized correctly
    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    assert!(Gear::<T>::is_active(&builtins, pid));
    assert!(Gear::<T>::is_initialized(pid));

    // Save signal code to be compared with
    let signal_code = SimpleExecutionError::StackLimitExceeded.into();
    assert_ok!(Gear::<T>::send_message(
        RawOrigin::Signed(origin.clone()).into(),
        pid,
        HandleAction::SaveSignal(signal_code).encode(),
        GAS_LIMIT,
        Zero::zero(),
        false,
    ));

    utils::run_to_next_block::<T>(None);

    // Send the action to trigger signal sending
    let next_user_mid = utils::get_next_message_id::<T>(origin.clone());
    assert_ok!(Gear::<T>::send_message(
        RawOrigin::Signed(origin.clone()).into(),
        pid,
        HandleAction::ExceedStackLimit.encode(),
        GAS_LIMIT,
        Zero::zero(),
        false,
    ));

    // Assert that system reserve gas node is removed
    assert_ok!(GasHandlerOf::<T>::get_system_reserve(next_user_mid));

    utils::run_to_next_block::<T>(None);

    assert!(GasHandlerOf::<T>::get_system_reserve(next_user_mid).is_err());

    // Ensure that signal code sent is signal code we saved
    let ok_mails = MailboxOf::<T>::iter_key(origin)
        .filter(|(m, _)| m.payload_bytes() == true.encode())
        .count();
    assert_eq!(ok_mails, 1);
}

pub fn main_test<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    SyscallName::all().for_each(|syscall| {
        log::info!("run test for {syscall:?}");
        match syscall {
            SyscallName::Send => check_send::<T>(None),
            SyscallName::SendWGas => check_send::<T>(Some(25_000_000_000)),
            SyscallName::SendCommit => check_send_raw::<T>(None),
            SyscallName::SendCommitWGas => check_send_raw::<T>(Some(25_000_000_000)),
            SyscallName::SendInit | SyscallName:: SendPush => {/* skipped, due to test being run in SendCommit* variants */},
            SyscallName::SendInput => check_send_input::<T>(None),
            SyscallName::SendPushInput => check_send_push_input::<T>(),
            SyscallName::SendInputWGas => check_send_input::<T>(Some(25_000_000_000)),
            SyscallName::Reply => check_reply::<T>(None),
            SyscallName::ReplyWGas => check_reply::<T>(Some(25_000_000_000)),
            SyscallName::ReplyCommit => check_reply_raw::<T>(None),
            SyscallName::ReplyCommitWGas => check_reply_raw::<T>(Some(25_000_000_000)),
            SyscallName::ReplyTo => check_reply_details::<T>(),
            SyscallName::SignalFrom => check_signal_details::<T>(),
            SyscallName::ReplyPush => {/* skipped, due to test being run in SendCommit* variants */},
            SyscallName::ReplyInput => check_reply_input::<T>(None),
            SyscallName::ReplyPushInput => check_reply_push_input::<T>(),
            SyscallName::ReplyInputWGas => check_reply_input::<T>(Some(25_000_000_000)),
            SyscallName::CreateProgram => check_create_program::<T>(None),
            SyscallName::CreateProgramWGas => check_create_program::<T>(Some(25_000_000_000)),
            SyscallName::ReplyDeposit => check_gr_reply_deposit::<T>(),
            SyscallName::Read => {/* checked in all the calls internally */},
            SyscallName::Size => check_gr_size::<T>(),
            SyscallName::ReplyCode => {/* checked in reply_to */},
            SyscallName::SignalCode => {/* checked in signal_from */},
            SyscallName::MessageId => check_gr_message_id::<T>(),
            SyscallName::ProgramId => check_gr_program_id::<T>(),
            SyscallName::Source => check_gr_source::<T>(),
            SyscallName::Value => check_gr_value::<T>(),
            SyscallName::EnvVars => check_gr_env_vars::<T>(),
            SyscallName::BlockHeight => check_gr_block_height::<T>(),
            SyscallName::BlockTimestamp => check_gr_block_timestamp::<T>(),
            SyscallName::GasAvailable => check_gr_gas_available::<T>(),
            SyscallName::ValueAvailable => check_gr_value_available::<T>(),
            SyscallName::Exit
            | SyscallName::Leave
            | SyscallName::Wait
            | SyscallName::WaitFor
            | SyscallName::WaitUpTo
            | SyscallName::Wake
            | SyscallName::Debug
            | SyscallName::Panic
            | SyscallName::OomPanic => {/* tests here aren't required, read module docs for more info */},
            SyscallName::Alloc
            | SyscallName::Free
            | SyscallName::FreeRange => check_mem::<T>(),
            SyscallName::SystemBreak => {/* no need for tests because tested in other bench test */}
            SyscallName::Random => check_gr_random::<T>(),
            SyscallName::ReserveGas => check_gr_reserve_gas::<T>(),
            SyscallName::UnreserveGas => check_gr_unreserve_gas::<T>(),
            SyscallName::ReservationSend => check_gr_reservation_send::<T>(),
            SyscallName::ReservationSendCommit => check_gr_reservation_send_commit::<T>(),
            SyscallName::ReservationReply => check_gr_reservation_reply::<T>(),
            SyscallName::ReservationReplyCommit => check_gr_reservation_reply_commit::<T>(),
            SyscallName::SystemReserveGas => check_gr_system_reserve_gas::<T>(),
        }
    });
}

fn check_gr_system_reserve_gas<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_pid, _| {
        let reserve_amount = 10_000_000;
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());

        let post_check = move || {
            assert!(
                WaitlistOf::<T>::contains(&tester_pid, &next_user_mid),
                "wait list post check failed"
            );
            assert_eq!(
                Ok(reserve_amount),
                GasHandlerOf::<T>::get_system_reserve(next_user_mid),
                "system reserve gas post check failed"
            );
        };

        let mp = vec![Kind::SystemReserveGas(reserve_amount)].encode().into();

        (TestCall::send_message(mp), Some(post_check))
    });
}

fn check_gr_reply_deposit<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let deposit_amount = 10_000_000;
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());

        let outgoing_mid = MessageId::generate_outgoing(next_user_mid, 0);
        let future_reply_id = MessageId::generate_reply(outgoing_mid);

        let post_check = move || {
            assert!(
                GasHandlerOf::<T>::exists_and_deposit(future_reply_id),
                "gas tree post check failed"
            );
            assert_eq!(
                Ok(deposit_amount),
                GasHandlerOf::<T>::get_limit(future_reply_id),
                "reply deposit gas post check failed"
            );
        };

        let mp = vec![Kind::ReplyDeposit(deposit_amount)].encode().into();

        (TestCall::send_message(mp), Some(post_check))
    });
}

fn check_gr_reservation_send<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);

        let mp = vec![Kind::ReservationSend(expected_mid.into())]
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_reservation_send_commit<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let payload = b"HI_RSC!!";
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        // Program increases local nonce by sending one message before `send_init`.
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 1);

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_mid && m.payload_bytes() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = vec![Kind::ReservationSendRaw(
            payload.to_vec(),
            expected_mid.into(),
        )]
        .encode()
        .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

fn check_gr_reservation_reply<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        let mp = vec![Kind::ReservationReply(expected_mid.into())]
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_reservation_reply_commit<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let payload = b"HI_RRC!!";
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        let post_test = move || {
            let source = default_sender.cast();
            assert!(SystemPallet::<T>::events().into_iter().any(|e| {
                let bytes = e.event.encode();
                let Ok(gear_event): Result<Event<T>, _> = Event::decode(&mut bytes[1..].as_ref()) else { return false };
                matches!(gear_event, Event::UserMessageSent { message, .. } if message.id() == expected_mid && message.payload_bytes() == payload && message.destination() == source)
            }), "No message with expected id found in events");
        };

        let mp = vec![Kind::ReservationReplyCommit(
            payload.to_vec(),
            expected_mid.into(),
        )]
        .encode()
        .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

fn check_mem<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    #[cfg(feature = "std")]
    utils::init_logger();

    let wasm_module = alloc_free_test_wasm::<T>();

    let default_account = utils::default_account();
    let _ = CurrencyOf::<T>::deposit_creating(
        &default_account,
        100_000_000_000_000_u128.unique_saturated_into(),
    );

    // Set default code-hash for create program calls
    Gear::<T>::upload_program(
        RawOrigin::Signed(default_account.clone()).into(),
        wasm_module.code,
        b"alloc-free-test".to_vec(),
        b"".to_vec(),
        50_000_000_000,
        0u128.unique_saturated_into(),
        false,
    )
    .expect("failed to upload test program");

    let pid = ActorId::generate_from_user(wasm_module.hash, b"alloc-free-test");
    utils::run_to_next_block::<T>(None);

    // no errors occurred
    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    assert!(Gear::<T>::is_initialized(pid));
    assert!(Gear::<T>::is_active(&builtins, pid));
    assert!(MailboxOf::<T>::is_empty(&default_account));

    Gear::<T>::send_message(
        RawOrigin::Signed(default_account.clone()).into(),
        pid,
        b"".to_vec(),
        50_000_000_000,
        0u128.unique_saturated_into(),
        false,
    )
    .expect("failed to send message to test program");
    utils::run_to_next_block::<T>(None);

    // no errors occurred
    assert!(Gear::<T>::is_initialized(pid));
    assert!(Gear::<T>::is_active(&builtins, pid));
    assert!(MailboxOf::<T>::is_empty(&default_account));

    Gear::<T>::reset();
}

fn check_gr_size<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let expected_size = vec![Kind::Size(0)].encoded_size() as u32;

        let mp = vec![Kind::Size(expected_size)].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_message_id<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());

        let mp = vec![Kind::MessageId(next_user_mid.into())].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_program_id<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|id, _| {
        let mp = vec![Kind::ActorId(id.into())].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_source<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let message_sender = benchmarking::account::<T::AccountId>("some_user", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(
            &message_sender,
            50_000_000_000_000_u128.unique_saturated_into(),
        );
        let mp = MessageParamsBuilder::new(
            vec![Kind::Source(
                message_sender.clone().into_origin().to_fixed_bytes(),
            )]
            .encode(),
        )
        .with_sender(message_sender);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_value<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let sending_value = 10_000_000_000_000;

        let mp = MessageParamsBuilder::new(vec![Kind::Value(sending_value)].encode())
            .with_value(sending_value);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_value_available<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let sending_value = 20_000_000_000_000;
        // Program sends 10_000_000_000_000
        let mp = MessageParamsBuilder::new(vec![Kind::ValueAvailable(10_000_000_000_000)].encode())
            .with_value(sending_value);

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// Depending on `gas` param will be `gr_create_program` or `gr_create_program_wgas.
fn check_create_program<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);
        let salt = 10u64;
        let expected_pid = ActorId::generate_from_program(
            next_user_mid,
            simplest_gear_wasm::<T>().hash,
            &salt.to_le_bytes(),
        );

        let mp = MessageParamsBuilder::new(
            vec![Kind::CreateProgram(
                salt,
                gas,
                (expected_mid.into(), expected_pid.into()),
            )]
            .encode(),
        )
        .with_value(CurrencyOf::<T>::minimum_balance().unique_saturated_into());

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

// Depending on `gas` param will be `gr_send` or `gr_send_wgas`.
fn check_send<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);

        let payload = vec![Kind::Send(gas, expected_mid.into())].encode();
        log::debug!("payload = {payload:?}");
        let mp = payload.into();
        // log::debug!("mp = {mp:?}");

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

/// Tests send_init, send_push, send_commit or send_commit_wgas depending on `gas` param.
fn check_send_raw<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let payload = b"HI_SR!!";
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        // Program increases local nonce by sending messages twice before `send_init`.
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 2);

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_mid && m.payload_bytes() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = vec![Kind::SendRaw(payload.to_vec(), gas, expected_mid.into())]
            .encode()
            .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

// Depending on `gas` param will be `gr_send` or `gr_send_wgas`.
fn check_send_input<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_message_id = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_message_id = MessageId::generate_outgoing(next_message_id, 0);

        let payload = vec![Kind::SendInput(gas, expected_message_id.into())].encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload_bytes() == payload),
                "No message with expected id found in queue"
            );
        };

        (TestCall::send_message(message), Some(post_test))
    });
}

// Tests `send_init`, `send_push_input` and `send_commit`.
#[track_caller]
fn check_send_push_input<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_message_id = utils::get_next_message_id::<T>(default_sender.clone());
        // Program increases local nonce by sending messages twice before `send_init`.
        let expected_message_id = MessageId::generate_outgoing(next_message_id, 2);

        let payload = vec![Kind::SendPushInput(expected_message_id.into())].encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload_bytes() == payload),
                "No message with expected id found in queue"
            );
        };

        (TestCall::send_message(message), Some(post_test))
    });
}

// Depending on `gas` param will be `gr_reply` or `gr_reply_wgas`.
fn check_reply<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        let mp = vec![Kind::Reply(gas, expected_mid.into())].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// Tests `reply_push` and `reply_commit` or `reply_commit_wgas` depending on `gas` value.
fn check_reply_raw<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let payload = b"HI_RR!!";
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        let post_test = move || {
            let source = default_sender.cast();
            assert!(SystemPallet::<T>::events().into_iter().any(|e| {
                let bytes = e.event.encode();
                let Ok(gear_event): Result<Event<T>, _> = Event::decode(&mut bytes[1..].as_ref()) else { return false };
                matches!(gear_event, Event::UserMessageSent { message, .. } if message.id() == expected_mid && message.payload_bytes() == payload && message.destination() == source)
            }), "No message with expected id found in events");
        };

        let mp = vec![Kind::ReplyRaw(payload.to_vec(), gas, expected_mid.into())]
            .encode()
            .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

// Tests `reply_input` or `reply_input_wgas` depending on `gas` value.
fn check_reply_input<T>(gas: Option<u64>)
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_message_id = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_message_id = MessageId::generate_reply(next_message_id);

        let payload = vec![Kind::ReplyInput(gas, expected_message_id.into())].encode();
        let message = payload.clone().into();

        let post_test = move || {
            let source = default_sender.cast();
            assert!(SystemPallet::<T>::events().into_iter().any(|e| {
                let bytes = e.event.encode();
                let Ok(gear_event): Result<Event<T>, _> = Event::decode(&mut bytes[1..].as_ref()) else { return false };
                matches!(gear_event, Event::UserMessageSent { message, .. } if message.id() == expected_message_id && message.payload_bytes() == payload && message.destination() == source)
            }), "No message with expected id found in events");
        };

        (TestCall::send_message(message), Some(post_test))
    });
}

// Tests `reply_push_input` and `reply_commit`.
fn check_reply_push_input<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_message_id = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_message_id = MessageId::generate_reply(next_message_id);

        let payload = vec![Kind::ReplyPushInput(expected_message_id.into())].encode();
        let message = payload.clone().into();

        let post_test = move || {
            let source = default_sender.cast();
            assert!(SystemPallet::<T>::events().into_iter().any(|e| {
                let bytes = e.event.encode();
                let Ok(gear_event): Result<Event<T>, _> = Event::decode(&mut bytes[1..].as_ref()) else { return false };
                matches!(gear_event, Event::UserMessageSent { message, .. } if message.id() == expected_message_id && message.payload_bytes() == payload && message.destination() == source)
            }), "No message with expected id found in events");
        };

        (TestCall::send_message(message), Some(post_test))
    });
}

// Tests `gr_reply_to` and `gr_reply_code`
fn check_reply_details<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_pid, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);

        let reply_code = ReplyCode::Success(SuccessReplyReason::Manual).to_bytes();

        // trigger sending message to default_sender's mailbox
        Gear::<T>::send_message(
            RawOrigin::Signed(default_sender.clone()).into(),
            tester_pid,
            // random params in ReplyDetails, because they aren't checked
            vec![Kind::ReplyDetails([255u8; 32], reply_code)].encode(),
            50_000_000_000,
            0u128.unique_saturated_into(),
            false,
        )
        .expect("triggering message send to mailbox failed");

        utils::run_to_next_block::<T>(None);

        let reply_to = MailboxOf::<T>::iter_key(default_sender)
            .last()
            .map(|(m, _)| m)
            .expect("no mail found after invoking syscall test program");

        assert_eq!(reply_to.id(), expected_mid, "mailbox check failed");

        let mp =
            MessageParamsBuilder::new(Kind::ReplyDetails(expected_mid.into(), reply_code).encode())
                .with_reply_id(reply_to.id());

        (TestCall::send_reply(mp), None::<DefaultPostCheck>)
    });
}

// Tests `gr_signal_from` and `gr_signal_code`
fn check_signal_details<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_pid, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_outgoing(next_user_mid, 0);

        // setup signal details
        Gear::<T>::send_message(
            RawOrigin::Signed(default_sender.clone()).into(),
            tester_pid,
            vec![Kind::SignalDetails].encode(),
            50_000_000_000,
            0u128.unique_saturated_into(),
            false,
        )
        .expect("triggering message send to mailbox failed");

        utils::run_to_next_block::<T>(None);

        let reply_to = MailboxOf::<T>::iter_key(default_sender)
            .last()
            .map(|(m, _)| m)
            .expect("no mail found after invoking syscall test program");

        assert_eq!(reply_to.id(), expected_mid, "mailbox check failed");

        let mp = MessageParamsBuilder::new(Kind::SignalDetailsWake.encode())
            .with_reply_id(reply_to.id());

        (TestCall::send_reply(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_env_vars<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let performance_multiplier = T::PerformanceMultiplier::get().value();
        let existential_deposit = T::Currency::minimum_balance().unique_saturated_into();
        let mailbox_threshold = T::MailboxThreshold::get();
        let gas_to_value_multiplier = <T as pallet_gear_bank::Config>::GasMultiplier::get()
            .gas_to_value(1)
            .unique_saturated_into();
        let mp = vec![Kind::EnvVars {
            performance_multiplier,
            existential_deposit,
            mailbox_threshold,
            gas_to_value_multiplier,
        }]
        .encode()
        .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_block_height<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let current_height: u32 = SystemPallet::<T>::block_number().unique_saturated_into();
        let height_delta = 15;
        utils::run_to_block::<T>(current_height + height_delta, None);

        let mp = vec![Kind::BlockHeight(current_height + height_delta + 1)]
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_block_timestamp<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        // will remain constant
        let block_timestamp = 125;
        TimestampPallet::<T>::set(
            RawOrigin::None.into(),
            block_timestamp.unique_saturated_into(),
        )
        .expect("failed to put timestamp");

        let mp = vec![Kind::BlockTimestamp(block_timestamp)].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_reserve_gas<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_user_mid =
            utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        // Nonce in program is set to 2 due to 3 times reservation is called.
        let expected_reservation_id = ReservationId::generate(next_user_mid, 2).encode();
        let mp = vec![Kind::Reserve(expected_reservation_id)].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_unreserve_gas<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let mp = vec![Kind::Unreserve(10_000)].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_random<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let next_mid = utils::get_next_message_id::<T>(utils::default_account::<T::AccountId>());
        let (random, expected_bn) = T::Randomness::random(next_mid.as_ref());

        // If we use vara-runtime, current epoch starts at block 0,
        // But mock runtime will reference currently proceeding block number,
        // so we add to currently got value.
        #[cfg(feature = "std")]
        let expected_bn = expected_bn + One::one();

        let salt = [1; 32];
        let expected_hash = {
            // Internals of the gr_random call
            let mut salt_vec = salt.to_vec();
            salt_vec.extend_from_slice(random.as_ref());

            sp_io::hashing::blake2_256(&salt_vec)
        };

        let mp = vec![Kind::Random(
            salt,
            (expected_hash, expected_bn.unique_saturated_into()),
        )]
        .encode()
        .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// TODO: although we do not want to test the business logic,
// this test is still unstable due to constants #4030
fn check_gr_gas_available<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        // Expected to burn not more than 750_000_000
        // Provided gas in the test by default is 50_000_000_000
        let lower = 50_000_000_000 - 750_000_000;
        let upper = 50_000_000_000 - 100_000_000;
        let mp = vec![Kind::GasAvailable(lower, upper)].encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn run_tester<T, P, S, Id>(get_test_call_params: S)
where
    T: Config + frame_system::Config<AccountId = Id>,
    // T::AccountId: Origin,
    T::RuntimeOrigin: From<RawOrigin<Id>>,
    Id: Clone + Origin + FullCodec,
    // Post check
    P: FnOnce(),
    // Get syscall and post check
    S: FnOnce(ActorId, CodeId) -> (TestCall<Id>, Option<P>),
{
    #[cfg(feature = "std")]
    utils::init_logger();

    let child_wasm = simplest_gear_wasm::<T>();
    let child_code = child_wasm.code;
    let child_code_hash = child_wasm.hash;
    let child_pid = ActorId::generate_from_user(child_code_hash, b"");

    let tester_pid = ActorId::generate_from_user(CodeId::generate(SYSCALLS_TEST_WASM_BINARY), b"");

    // Deploy program with valid code hash
    let child_deployer = benchmarking::account::<T::AccountId>("child_deployer", 0, 0);
    let _ = CurrencyOf::<T>::deposit_creating(
        &child_deployer,
        100_000_000_000_000_u128.unique_saturated_into(),
    );
    Gear::<T>::upload_program(
        RawOrigin::Signed(child_deployer).into(),
        child_code,
        vec![],
        vec![],
        50_000_000_000,
        0u128.unique_saturated_into(),
        false,
    )
    .expect("child program deploy failed");

    // Set default code-hash for create program calls
    let default_account = utils::default_account();
    let _ = CurrencyOf::<T>::deposit_creating(
        &default_account,
        100_000_000_000_000_u128.unique_saturated_into(),
    );
    Gear::<T>::upload_program(
        RawOrigin::Signed(default_account).into(),
        SYSCALLS_TEST_WASM_BINARY.to_vec(),
        b"".to_vec(),
        child_code_hash.encode(),
        50_000_000_000,
        0u128.unique_saturated_into(),
        false,
    )
    .expect("syscall check program deploy failed");

    utils::run_to_next_block::<T>(None);

    let (call, post_check) = get_test_call_params(tester_pid, child_code_hash);
    let sender;
    match call {
        TestCall::SendMessage(mp) => {
            sender = mp.sender.clone();
            Gear::<T>::send_message(
                RawOrigin::Signed(mp.sender).into(),
                tester_pid,
                mp.payload,
                50_000_000_000,
                mp.value.unique_saturated_into(),
                false,
            )
            .expect("failed send message");
        }
        TestCall::SendReply(rp) => {
            sender = rp.sender.clone();
            Gear::<T>::send_reply(
                RawOrigin::Signed(rp.sender).into(),
                rp.reply_to_id,
                rp.payload,
                50_000_000_000,
                rp.value.unique_saturated_into(),
                false,
            )
            .expect("failed send reply");
        }
    }

    // Main check
    // let user_mid = get_last_message_id();
    utils::run_to_next_block::<T>(None);
    let ok_mails = MailboxOf::<T>::iter_key(sender)
        .filter(|(m, _)| m.payload_bytes() == b"ok")
        .count();
    assert_eq!(ok_mails, 1);

    // Optional post-main check
    if let Some(post_check) = post_check {
        post_check();
    }

    // Manually reset the storage
    Gear::<T>::reset();
    let tester_account_id = tester_pid.cast();
    let _ = CurrencyOf::<T>::slash(
        &tester_account_id,
        CurrencyOf::<T>::free_balance(&tester_account_id),
    );
    frame_system::pallet::Account::<T>::remove(tester_account_id);
    frame_system::pallet::Account::<T>::remove(child_pid.cast::<T::AccountId>());
}

type DefaultPostCheck = fn() -> ();

enum TestCall<Id> {
    SendMessage(SendMessageParams<Id>),
    SendReply(SendReplyParams<Id>),
}

impl<Id: Origin> TestCall<Id> {
    fn send_message(mp: MessageParamsBuilder<Id>) -> Self {
        TestCall::SendMessage(mp.build_send_message())
    }

    fn send_reply(mp: MessageParamsBuilder<Id>) -> Self {
        TestCall::SendReply(mp.build_send_reply())
    }
}

struct SendMessageParams<Id> {
    sender: Id,
    payload: Vec<u8>,
    value: u128,
}

struct SendReplyParams<Id> {
    sender: Id,
    reply_to_id: MessageId,
    payload: Vec<u8>,
    value: u128,
}

struct MessageParamsBuilder<Id> {
    sender: Id,
    payload: Vec<u8>,
    value: Option<u128>,
    reply_to_id: Option<MessageId>,
}

impl<Id: Origin> MessageParamsBuilder<Id> {
    fn with_sender(mut self, sender: Id) -> Self {
        self.sender = sender;
        self
    }

    fn with_value(mut self, value: u128) -> Self {
        self.value = Some(value);
        self
    }

    fn with_reply_id(mut self, reply_to_id: MessageId) -> Self {
        self.reply_to_id = Some(reply_to_id);
        self
    }

    fn build_send_message(self) -> SendMessageParams<Id> {
        let MessageParamsBuilder {
            sender,
            payload,
            value,
            ..
        } = self;
        SendMessageParams {
            sender,
            payload,
            value: value.unwrap_or(0),
        }
    }

    fn build_send_reply(self) -> SendReplyParams<Id> {
        let MessageParamsBuilder {
            sender,
            payload,
            value,
            reply_to_id,
        } = self;
        SendReplyParams {
            sender,
            reply_to_id: reply_to_id.expect("internal error: reply id wasn't set"),
            payload,
            value: value.unwrap_or(0),
        }
    }
}

impl<Id: Origin> MessageParamsBuilder<Id> {
    fn new(payload: Vec<u8>) -> Self {
        let sender = utils::default_account();
        Self {
            payload,
            sender,
            value: None,
            reply_to_id: None,
        }
    }
}

impl<Id: Origin> From<Vec<u8>> for MessageParamsBuilder<Id> {
    fn from(v: Vec<u8>) -> Self {
        MessageParamsBuilder::new(v)
    }
}

// (module
//     (import "env" "memory" (memory 1))
//     (export "handle" (func $handle))
//     (export "init" (func $init))
//     (func $handle)
//     (func $init)
// )
fn simplest_gear_wasm<T: Config>() -> WasmModule<T>
where
    T::AccountId: Origin,
{
    ModuleDefinition {
        memory: Some(ImportedMemory::new(1)),
        ..Default::default()
    }
    .into()
}

fn alloc_free_test_wasm<T: Config>() -> WasmModule<T>
where
    T::AccountId: Origin,
{
    ModuleDefinition {
        memory: Some(ImportedMemory::new(1)),
        imported_functions: vec![
            SyscallName::Alloc,
            SyscallName::Free,
            SyscallName::FreeRange,
        ],
        init_body: Some(Function::from_instructions([
            // allocate 5 pages
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x5),
            Instruction::Call(0),
            Instruction::I32Const(0x1),
            Instruction::I32Eq,
            Instruction::BrIf(0),
            Instruction::Unreachable,
            Instruction::End,
            // put some values in pages 2-5
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x10001),
            Instruction::I32Const(0x61),
            Instruction::I32Store(MemArg::i32()),
            Instruction::I32Const(0x20001),
            Instruction::I32Const(0x62),
            Instruction::I32Store(MemArg::i32()),
            Instruction::I32Const(0x30001),
            Instruction::I32Const(0x63),
            Instruction::I32Store(MemArg::i32()),
            Instruction::I32Const(0x40001),
            Instruction::I32Const(0x64),
            Instruction::I32Store(MemArg::i32()),
            Instruction::End,
            // check it has the value
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x10001),
            Instruction::I32Load(MemArg::i32()),
            Instruction::I32Const(0x61),
            Instruction::I32Eq,
            Instruction::BrIf(0),
            Instruction::Unreachable,
            Instruction::End,
            // free second page
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x1),
            Instruction::Call(1),
            Instruction::Drop,
            Instruction::End,
            // free_range pages 2-4
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x1),
            Instruction::I32Const(0x3),
            Instruction::Call(2),
            Instruction::Drop,
            Instruction::End,
            Instruction::End,
        ])),
        handle_body: Some(Function::from_instructions([
            // check that the second page is empty
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x10001),
            Instruction::I32Load(MemArg::i32()),
            Instruction::I32Const(0x0),
            Instruction::I32Eq,
            Instruction::BrIf(0),
            Instruction::Unreachable,
            Instruction::End,
            // check that the 3rd page is empty
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x20001),
            Instruction::I32Load(MemArg::i32()),
            Instruction::I32Const(0x0),
            Instruction::I32Eq,
            Instruction::BrIf(0),
            Instruction::Unreachable,
            Instruction::End,
            // check that the 5th page still has data
            Instruction::Block(BlockType::Empty),
            Instruction::I32Const(0x40001),
            Instruction::I32Load(MemArg::i32()),
            Instruction::I32Const(0x64),
            Instruction::I32Eq,
            Instruction::BrIf(0),
            Instruction::Unreachable,
            Instruction::End,
            Instruction::End,
        ])),
        ..Default::default()
    }
    .into()
}
