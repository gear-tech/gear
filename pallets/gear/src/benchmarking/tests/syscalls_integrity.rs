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

//! Testing integration level of sys-calls
//!
//! Integration level is the level between the user (`gcore`/`gstd`) and `core-backend`.
//! Tests here does not check complex business logic, but only the fact that all the
//! requested data is received properly, i.e., pointers receive expected types, no export func
//! signature map errors.
//!
//! `gr_read` is tested in the `test_syscall` program by calling `msg::load` to decode each sys-call type.
//! `gr_exit` and `gr_wait*` call are not intended to be tested with the integration level tests, but only
//! with business logic tests in the separate module.

use super::*;

use crate::WaitlistOf;
use frame_support::traits::Randomness;
use gear_core::ids::{CodeId, ReservationId};
use gear_core_errors::{ExtError, MessageError};
use gear_wasm_instrument::syscalls::SysCallName;
use pallet_timestamp::Pallet as TimestampPallet;
use test_syscalls::{Kind, WASM_BINARY as SYSCALLS_TEST_WASM_BINARY};

pub fn main_test<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    SysCallName::all().for_each(|sys_call| {
        log::info!("run test for {sys_call:?}");
        match sys_call {
            SysCallName::Send => check_send::<T>(None),
            SysCallName::SendWGas => check_send::<T>(Some(25_000_000_000)),
            SysCallName::SendCommit => check_send_raw::<T>(None),
            SysCallName::SendCommitWGas => check_send_raw::<T>(Some(25_000_000_000)),
            SysCallName::SendInit | SysCallName:: SendPush => {/* skipped, due to test being run in SendCommit* variants */},
            SysCallName::SendInput => check_send_input::<T>(None),
            SysCallName::SendPushInput => check_send_push_input::<T>(),
            SysCallName::SendInputWGas => check_send_input::<T>(Some(25_000_000_000)),
            SysCallName::Reply => check_reply::<T>(None),
            SysCallName::ReplyWGas => check_reply::<T>(Some(25_000_000_000)),
            SysCallName::ReplyCommit => check_reply_raw::<T>(None),
            SysCallName::ReplyCommitWGas => check_reply_raw::<T>(Some(25_000_000_000)),
            SysCallName::ReplyTo => check_reply_details::<T>(),
            SysCallName::SignalFrom => check_signal_details::<T>(),
            SysCallName::ReplyPush => {/* skipped, due to test being run in SendCommit* variants */},
            SysCallName::ReplyInput => check_reply_input::<T>(None),
            SysCallName::ReplyPushInput => check_reply_push_input::<T>(),
            SysCallName::ReplyInputWGas => check_reply_input::<T>(Some(25_000_000_000)),
            SysCallName::CreateProgram => check_create_program::<T>(None),
            SysCallName::CreateProgramWGas => check_create_program::<T>(Some(25_000_000_000)),
            SysCallName::Read => {/* checked in all the calls internally */},
            SysCallName::Size => check_gr_size::<T>(),
            SysCallName::StatusCode => {/* checked in reply_to */},
            SysCallName::MessageId => check_gr_message_id::<T>(),
            SysCallName::ProgramId => check_gr_program_id::<T>(),
            SysCallName::Source => check_gr_source::<T>(),
            SysCallName::Value => check_gr_value::<T>(),
            SysCallName::BlockHeight => check_gr_block_height::<T>(),
            SysCallName::BlockTimestamp => check_gr_block_timestamp::<T>(),
            SysCallName::Origin => check_gr_origin::<T>(),
            SysCallName::GasAvailable => check_gr_gas_available::<T>(),
            SysCallName::ValueAvailable => check_gr_value_available::<T>(),
            SysCallName::Exit
            | SysCallName::Leave
            | SysCallName::Wait
            | SysCallName::WaitFor
            | SysCallName::WaitUpTo
            | SysCallName::Wake
            | SysCallName::Debug
            | SysCallName::Panic
            | SysCallName::OomPanic => {/* tests here aren't required, read module docs for more info */},
            SysCallName::Alloc => check_mem::<T>(false),
            SysCallName::Free => check_mem::<T>(true),
            SysCallName::OutOfGas | SysCallName::OutOfAllowance => { /*no need for tests */}
            SysCallName::Error => check_gr_err::<T>(),
            SysCallName::Random => check_gr_random::<T>(),
            SysCallName::ReserveGas => check_gr_reserve_gas::<T>(),
            SysCallName::UnreserveGas => check_gr_unreserve_gas::<T>(),
            SysCallName::ReservationSend => check_gr_reservation_send::<T>(),
            SysCallName::ReservationSendCommit => check_gr_reservation_send_commit::<T>(),
            SysCallName::ReservationReply => check_gr_reservation_reply::<T>(),
            SysCallName::ReservationReplyCommit => check_gr_reservation_reply_commit::<T>(),
            SysCallName::SystemReserveGas => check_gr_system_reserve_gas::<T>(),
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

        let mp = Kind::SystemReserveGas(reserve_amount).encode().into();

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

        let mp = Kind::ReservationSend(expected_mid.into()).encode().into();

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
                    .any(|(m, _)| m.id() == expected_mid && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::ReservationSendRaw(payload.to_vec(), expected_mid.into())
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

        let mp = Kind::ReservationReply(expected_mid.into()).encode().into();

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
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_mid && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::ReservationReplyCommit(payload.to_vec(), expected_mid.into())
            .encode()
            .into();

        (TestCall::send_message(mp), Some(post_test))
    });
}

fn check_mem<T>(check_free: bool)
where
    T: Config,
    T::AccountId: Origin,
{
    #[cfg(feature = "std")]
    utils::init_logger();

    let wasm_module = alloc_free_test_wasm::<T>();

    let default_account = utils::default_account();
    <T as pallet::Config>::Currency::deposit_creating(
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
    )
    .expect("failed to upload test program");

    let pid = ProgramId::generate(wasm_module.hash, b"alloc-free-test");
    utils::run_to_next_block::<T>(None);

    // no errors occurred
    assert!(MailboxOf::<T>::is_empty(&default_account));

    if check_free {
        Gear::<T>::send_message(
            RawOrigin::Signed(default_account.clone()).into(),
            pid,
            b"".to_vec(),
            50_000_000_000,
            0u128.unique_saturated_into(),
        )
        .expect("failed to send message to test program");
        utils::run_to_next_block::<T>(None);

        // no errors occurred
        assert!(MailboxOf::<T>::is_empty(&default_account));
    }

    Gear::<T>::reset();
}

fn check_gr_err<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let message_value = u128::MAX;
        let expected_err = ExtError::Message(MessageError::NotEnoughValue {
            message_value,
            value_left: 0,
        });
        let expected_err = ::alloc::format!("API error: {expected_err}");

        let mp = Kind::Error(message_value, expected_err).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    });
}

fn check_gr_size<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        // One byte for enum variant, four bytes for u32 value
        let expected_size = 5;

        let mp = Kind::Size(expected_size).encode().into();

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

        let mp = Kind::MessageId(next_user_mid.into()).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_program_id<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|id, _| {
        let mp = Kind::ProgramId(id.into()).encode().into();

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
        <T as pallet::Config>::Currency::deposit_creating(
            &message_sender,
            50_000_000_000_000_u128.unique_saturated_into(),
        );
        let mp = MessageParamsBuilder::new(
            Kind::Source(message_sender.clone().into_origin().to_fixed_bytes()).encode(),
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
        let sending_value = u16::MAX as u128;
        let mp = MessageParamsBuilder::new(Kind::Value(sending_value).encode())
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
        let sending_value = 10_000;
        // Program sends 2000
        let mp = MessageParamsBuilder::new(Kind::ValueAvailable(sending_value - 2000).encode())
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
        let expected_pid = ProgramId::generate(simplest_gear_wasm::<T>().hash, &salt.to_le_bytes());

        let mp = Kind::CreateProgram(salt, gas, (expected_mid.into(), expected_pid.into()))
            .encode()
            .into();

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

        let mp = Kind::Send(gas, expected_mid.into()).encode().into();

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
                    .any(|(m, _)| m.id() == expected_mid && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::SendRaw(payload.to_vec(), gas, expected_mid.into())
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

        let payload = Kind::SendInput(gas, expected_message_id.into()).encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload() == payload),
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

        let payload = Kind::SendPushInput(expected_message_id.into()).encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload() == payload),
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

        let mp = Kind::Reply(gas, expected_mid.into()).encode().into();

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
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_mid && m.payload() == payload.to_vec()),
                "No message with expected id found in queue"
            );
        };

        let mp = Kind::ReplyRaw(payload.to_vec(), gas, expected_mid.into())
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

        let payload = Kind::ReplyInput(gas, expected_message_id.into()).encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload() == payload),
                "No message with expected id found in queue"
            );
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

        let payload = Kind::ReplyPushInput(expected_message_id.into()).encode();
        let message = payload.clone().into();

        let post_test = move || {
            assert!(
                MailboxOf::<T>::iter_key(default_sender)
                    .any(|(m, _)| m.id() == expected_message_id && m.payload() == payload),
                "No message with expected id found in queue"
            );
        };

        (TestCall::send_message(message), Some(post_test))
    });
}

// Tests `gr_reply_to` and  `gr_status_code`
fn check_reply_details<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_pid, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        // trigger sending message to default_sender's mailbox
        Gear::<T>::send_message(
            RawOrigin::Signed(default_sender.clone()).into(),
            tester_pid,
            // random params in ReplyDetails, because they aren't checked
            Kind::ReplyDetails([255u8; 32], 0).encode(),
            50_000_000_000,
            0u128.unique_saturated_into(),
        )
        .expect("triggering message send to mailbox failed");

        utils::run_to_next_block::<T>(None);

        let reply_to = MailboxOf::<T>::iter_key(default_sender)
            .last()
            .map(|(m, _)| m)
            .expect("no mail found after invoking sys-call test program");

        assert_eq!(reply_to.id(), expected_mid, "mailbox check failed");

        let mp = MessageParamsBuilder::new(Kind::ReplyDetails(expected_mid.into(), 0).encode())
            .with_reply_id(reply_to.id());

        (TestCall::send_reply(mp), None::<DefaultPostCheck>)
    });
}

// Tests `gr_signal_from` and `gr_status_code`
fn check_signal_details<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_pid, _| {
        let default_sender = utils::default_account::<T::AccountId>();
        let next_user_mid = utils::get_next_message_id::<T>(default_sender.clone());
        let expected_mid = MessageId::generate_reply(next_user_mid);

        // setup signal details
        Gear::<T>::send_message(
            RawOrigin::Signed(default_sender.clone()).into(),
            tester_pid,
            Kind::SignalDetails.encode(),
            50_000_000_000,
            0u128.unique_saturated_into(),
        )
        .expect("triggering message send to mailbox failed");

        utils::run_to_next_block::<T>(None);

        let reply_to = MailboxOf::<T>::iter_key(default_sender)
            .last()
            .map(|(m, _)| m)
            .expect("no mail found after invoking sys-call test program");

        assert_eq!(reply_to.id(), expected_mid, "mailbox check failed");

        let mp = MessageParamsBuilder::new(Kind::SignalDetailsWake.encode())
            .with_reply_id(reply_to.id());

        (TestCall::send_reply(mp), None::<DefaultPostCheck>)
    });
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

        let mp = Kind::BlockHeight(current_height + height_delta + 1)
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

        let mp = Kind::BlockTimestamp(block_timestamp).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_origin<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|tester_id, _| {
        use demo_proxy::{InputArgs, WASM_BINARY as PROXY_WASM_BINARY};

        let default_sender = utils::default_account::<T::AccountId>();
        let message_sender = benchmarking::account::<T::AccountId>("some_user", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(
            &message_sender,
            100_000_000_000_000_u128.unique_saturated_into(),
        );

        let payload = Kind::Origin(message_sender.clone().into_origin().to_fixed_bytes()).encode();

        // Upload proxy
        Gear::<T>::upload_program(
            RawOrigin::Signed(default_sender).into(),
            PROXY_WASM_BINARY.to_vec(),
            b"".to_vec(),
            InputArgs {
                destination: tester_id.into_origin().into(),
            }
            .encode(),
            50_000_000_000,
            0u128.unique_saturated_into(),
        )
        .expect("failed deploying proxy");
        let proxy_pid = ProgramId::generate(CodeId::generate(PROXY_WASM_BINARY), b"");
        utils::run_to_next_block::<T>(None);

        // Set origin in the tester program through origin
        Gear::<T>::send_message(
            RawOrigin::Signed(message_sender.clone()).into(),
            proxy_pid,
            payload.clone(),
            50_000_000_000,
            0u128.unique_saturated_into(),
        )
        .expect("failed setting origin");
        utils::run_to_next_block::<T>(None);

        // Check the origin
        let mp = MessageParamsBuilder::new(payload).with_sender(message_sender);

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
        let mp = Kind::Reserve(expected_reservation_id).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn check_gr_unreserve_gas<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        let mp = Kind::Unreserve(10_000).encode().into();

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

        // If we use gear-runtime, current epoch starts at block 0,
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

        let mp = Kind::Random(salt, (expected_hash, expected_bn.unique_saturated_into()))
            .encode()
            .into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

// TODO although we do not want to test the business logic,
// this test is still unstable due to constants
fn check_gr_gas_available<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    run_tester::<T, _, _, T::AccountId>(|_, _| {
        // Expected to burn not more than 750_000_000
        // Provided gas in the test by default is 50_000_000_000
        let lower = 50_000_000_000 - 1_000_000_000;
        let upper = 50_000_000_000 - 200_000_000;
        let mp = Kind::GasAvailable(lower, upper).encode().into();

        (TestCall::send_message(mp), None::<DefaultPostCheck>)
    })
}

fn run_tester<T, P, S, Id>(get_test_call_params: S)
where
    T: Config + frame_system::Config<AccountId = Id>,
    // T::AccountId: Origin,
    T::RuntimeOrigin: From<RawOrigin<Id>>,
    Id: Clone + Origin,
    // Post check
    P: FnOnce(),
    // Get sys call and post check
    S: FnOnce(ProgramId, CodeId) -> (TestCall<Id>, Option<P>),
{
    #[cfg(feature = "std")]
    utils::init_logger();

    let child_wasm = simplest_gear_wasm::<T>();
    let child_code = child_wasm.code;
    let child_code_hash = child_wasm.hash;

    let tester_pid = ProgramId::generate(CodeId::generate(SYSCALLS_TEST_WASM_BINARY), b"");

    // Deploy program with valid code hash
    let child_deployer = benchmarking::account::<T::AccountId>("child_deployer", 0, 0);
    <T as pallet::Config>::Currency::deposit_creating(
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
    )
    .expect("child program deploy failed");

    // Set default code-hash for create program calls
    let default_account = utils::default_account();
    <T as pallet::Config>::Currency::deposit_creating(
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
    )
    .expect("sys-call check program deploy failed");

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
            )
            .expect("failed send reply");
        }
    }

    // Main check
    // let user_mid = get_last_message_id();
    utils::run_to_next_block::<T>(None);
    let ok_mails = MailboxOf::<T>::iter_key(sender)
        .filter(|(m, _)| m.payload() == b"ok")
        .count();
    assert_eq!(ok_mails, 1);

    // Optional post-main check
    if let Some(post_check) = post_check {
        post_check();
    }

    // Manually reset the storage
    Gear::<T>::reset();
    <T as pallet::Config>::Currency::slash(
        &Id::from_origin(tester_pid.into_origin()),
        <T as pallet::Config>::Currency::free_balance(&Id::from_origin(tester_pid.into_origin())),
    );
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

// (module
//     (import "env" "memory" (memory 1))
//     (import "env" "alloc" (func $alloc (param i32) (result i32)))
//     (import "env" "free" (func $free (param i32)))
//     (export "init" (func $init))
//     (export "handle" (func $handle))
//     (func $init
//         ;; allocate 2 more pages with expected starting index 1
//         (block
//             i32.const 0x2
//             call $alloc
//             i32.const 0x1
//             i32.eq
//             br_if 0
//             unreachable
//         )
//         ;; put to page with index 2 (the third) some value
//         (block
//             i32.const 0x20001
//             i32.const 0x63
//             i32.store
//         )
//         ;; put to page with index 1 (the second) some value
//         (block
//             i32.const 0x10001
//             i32.const 0x64
//             i32.store
//         )
//         ;; check it has the value
//         (block
//             i32.const 0x10001
//             i32.load
//             i32.const 0x65
//             i32.eq
//             br_if 0
//             unreachable
//         )
//         ;; remove page with index 1 (the second page)
//         (block
//             i32.const 0x1
//             call $free
//         )
//     )
//     (func $handle
//         ;; check that the second page is empty
//         (block
//             i32.const 0x10001
//             i32.load
//             i32.const 0x0
//             i32.eq
//             br_if 0
//             unreachable
//         )
//         ;; check that the third page has data
//         (block
//             i32.const 0x20001
//             i32.load
//             i32.const 0x63
//             i32.eq
//             br_if 0
//             unreachable
//         )
//     )
// )
fn alloc_free_test_wasm<T: Config>() -> WasmModule<T>
where
    T::AccountId: Origin,
{
    use gear_wasm_instrument::parity_wasm::elements::{FuncBody, Instructions};

    ModuleDefinition {
        memory: Some(ImportedMemory::new(1)),
        imported_functions: vec![SysCallName::Alloc, SysCallName::Free],
        init_body: Some(FuncBody::new(
            vec![],
            Instructions::new(vec![
                // ;; allocate 2 more pages with expected starting index 1
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x2),
                Instruction::Call(0),
                Instruction::I32Const(0x1),
                Instruction::I32Eq,
                Instruction::BrIf(0),
                Instruction::Unreachable,
                Instruction::End,
                // ;; put to page with index 2 (the third) some value
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x20001),
                Instruction::I32Const(0x63),
                Instruction::I32Store(2, 0),
                Instruction::End,
                // ;; put to page with index 1 (the second) some value
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x10001),
                Instruction::I32Const(0x64),
                Instruction::I32Store(2, 0),
                Instruction::End,
                // ;; check it has the value
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x10001),
                Instruction::I32Load(2, 0),
                Instruction::I32Const(0x64),
                Instruction::I32Eq,
                Instruction::BrIf(0),
                Instruction::Unreachable,
                Instruction::End,
                // ;; remove page with index 1 (the second page)
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x1),
                Instruction::Call(1),
                Instruction::Drop,
                Instruction::End,
                Instruction::End,
            ]),
        )),
        handle_body: Some(FuncBody::new(
            vec![],
            Instructions::new(vec![
                // ;; check that the second page is empty
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x10001),
                Instruction::I32Load(2, 0),
                Instruction::I32Const(0x0),
                Instruction::I32Eq,
                Instruction::BrIf(0),
                Instruction::Unreachable,
                Instruction::End,
                // ;; check that the third page has data
                Instruction::Block(BlockType::NoResult),
                Instruction::I32Const(0x20001),
                Instruction::I32Load(2, 0),
                Instruction::I32Const(0x63),
                Instruction::I32Eq,
                Instruction::BrIf(0),
                Instruction::Unreachable,
                Instruction::End,
                Instruction::End,
            ]),
        )),
        ..Default::default()
    }
    .into()
}
