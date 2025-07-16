// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

use super::*;
use gear_core::ids::ReservationId;

#[track_caller]
fn send_user_message_prepare<T>(delay: u32)
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_delayed_sender::WASM_BINARY;

    let caller = benchmarking::account("caller", 0, 0);
    let _ =
        CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        WASM_BINARY.to_vec(),
        salt,
        delay.encode(),
        100_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    Gear::<T>::process_queue(ExtManager::<T>::new(builtins));
}

#[track_caller]
pub(super) fn remove_gas_reservation<T>() -> (ActorId, ReservationId)
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_reserve_gas::{InitAction, WASM_BINARY};

    let caller = benchmarking::account("caller", 0, 0);
    let _ =
        CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        WASM_BINARY.to_vec(),
        salt,
        InitAction::Normal(vec![(50_000, 100)]).encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    Gear::<T>::process_queue(ExtManager::<T>::new(builtins));

    let program: ActiveProgram<_> = ProgramStorageOf::<T>::get_program(program_id)
        .expect("program should exist")
        .try_into()
        .expect("program should be active");

    (
        program_id,
        program
            .gas_reservation_map
            .first_key_value()
            .map(|(k, _v)| *k)
            .unwrap(),
    )
}

#[track_caller]
pub(super) fn send_user_message<T>() -> MessageId
where
    T: Config,
    T::AccountId: Origin,
{
    let delay = 1u32;
    send_user_message_prepare::<T>(delay);

    let task = TaskPoolOf::<T>::iter_prefix_keys(Gear::<T>::block_number() + delay.into())
        .next()
        .expect("task should be scheduled");
    let (message_id, to_mailbox) = match task {
        ScheduledTask::SendUserMessage {
            message_id,
            to_mailbox,
        } => (message_id, to_mailbox),
        _ => unreachable!("task should be SendUserMessage"),
    };
    assert!(to_mailbox);

    message_id
}

#[track_caller]
pub(super) fn send_dispatch<T>() -> MessageId
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_constructor::{Call, Calls, Scheme, WASM_BINARY};

    let caller = benchmarking::account("caller", 0, 0);
    let _ =
        CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY.to_vec(),
        salt,
        Scheme::empty().encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    let delay = 1u32;
    let calls = Calls::builder().add_call(Call::Send(
        <[u8; 32]>::from(program_id.into_origin()).into(),
        [].into(),
        Some(0u64.into()),
        0u128.into(),
        delay.into(),
    ));
    Gear::<T>::send_message(
        RawOrigin::Signed(caller).into(),
        program_id,
        calls.encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("failed to send message");

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    Gear::<T>::process_queue(ExtManager::<T>::new(builtins));

    let task = TaskPoolOf::<T>::iter_prefix_keys(Gear::<T>::block_number() + delay.into())
        .next()
        .unwrap();

    match task {
        ScheduledTask::SendDispatch(message_id) => message_id,
        _ => unreachable!(),
    }
}

#[track_caller]
pub(super) fn wake_message<T>() -> (ActorId, MessageId)
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_waiter::{Command, WASM_BINARY, WaitSubcommand};

    let caller = benchmarking::account("caller", 0, 0);
    let _ =
        CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY.to_vec(),
        salt,
        vec![],
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    let delay = 10u32;
    Gear::<T>::send_message(
        RawOrigin::Signed(caller).into(),
        program_id,
        Command::Wait(WaitSubcommand::WaitFor(delay)).encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("failed to send message");

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    Gear::<T>::process_queue(ExtManager::<T>::new(builtins));

    let task = TaskPoolOf::<T>::iter_prefix_keys(Gear::<T>::block_number() + delay.into())
        .next()
        .unwrap();
    let (_program_id, message_id) = match task {
        ScheduledTask::WakeMessage(program_id, message_id) => (program_id, message_id),
        _ => unreachable!(),
    };

    (program_id, message_id)
}

#[track_caller]
pub(super) fn remove_from_waitlist<T>() -> (ActorId, MessageId)
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_waiter::{Command, WASM_BINARY, WaitSubcommand};

    let caller = benchmarking::account("caller", 0, 0);
    let _ =
        CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY.to_vec(),
        salt,
        vec![],
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    Gear::<T>::send_message(
        RawOrigin::Signed(caller).into(),
        program_id,
        Command::Wait(WaitSubcommand::Wait).encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("failed to send message");

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    Gear::<T>::process_queue(ExtManager::<T>::new(builtins));

    let expiration = find_latest_event::<T, _, _>(|event| match event {
        Event::MessageWaited { expiration, .. } => Some(expiration),
        _ => None,
    })
    .expect("message should be waited");

    let task = TaskPoolOf::<T>::iter_prefix_keys(expiration)
        .next()
        .unwrap();
    let (_program_id, message_id) = match task {
        ScheduledTask::RemoveFromWaitlist(program_id, message_id) => (program_id, message_id),
        _ => unreachable!(),
    };

    (program_id, message_id)
}

#[track_caller]
pub(super) fn remove_from_mailbox<T>() -> (ActorId, MessageId)
where
    T: Config,
    T::AccountId: Origin,
{
    send_user_message_prepare::<T>(0u32);

    find_latest_event::<T, _, _>(|event| match event {
        Event::UserMessageSent { message, .. } => Some((message.destination(), message.id())),
        _ => None,
    })
    .expect("message should be sent")
}
