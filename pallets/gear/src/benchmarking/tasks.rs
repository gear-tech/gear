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
use crate::binaries;
use gear_core::ids::ReservationId;

#[track_caller]
fn send_user_message_prepare<T>(delay: u32)
where
    T: Config,
    T::AccountId: Origin,
{
    use binaries::demo_delayed_sender::WASM_BINARY_OPT;

    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        WASM_BINARY_OPT.to_vec(),
        salt,
        delay.encode(),
        100_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    Gear::<T>::process_queue(Default::default());
}

#[track_caller]
pub(super) fn pause_program_prepare<T>(c: u32, code: Vec<u8>) -> ProgramId
where
    T: Config,
    T::AccountId: Origin,
{
    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 400_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(&code), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        code,
        salt,
        b"init_payload".to_vec(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    Gear::<T>::process_queue(Default::default());

    let memory_page = {
        let mut page = PageBuf::new_zeroed();
        page[0] = 1;

        page
    };

    ProgramStorageOf::<T>::update_active_program(program_id, |program| {
        for i in 0..c {
            let page = GearPage::from(i as u16);
            ProgramStorageOf::<T>::set_program_page_data(
                program_id,
                program.memory_infix,
                page,
                memory_page.clone(),
            );
            program.pages_with_data.insert(page);
        }

        let wasm_pages = (c as usize * GEAR_PAGE_SIZE) / WASM_PAGE_SIZE;
        program.allocations =
            BTreeSet::from_iter((0..wasm_pages).map(|i| WasmPage::from(i as u16)));
    })
    .expect("program should exist");

    program_id
}

#[track_caller]
pub(super) fn remove_resume_session<T>() -> SessionId
where
    T: Config,
    T::AccountId: Origin,
{
    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());
    let code = benchmarking::generate_wasm(16.into()).unwrap();
    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(&code), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        code,
        salt,
        b"init_payload".to_vec(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    init_block::<T>(None);

    ProgramStorageOf::<T>::pause_program(program_id, 100u32.into()).unwrap();

    Gear::<T>::resume_session_init(
        RawOrigin::Signed(caller).into(),
        program_id,
        Default::default(),
        CodeId::default(),
    )
    .expect("failed to start resume session");

    get_last_session_id::<T>().unwrap()
}

#[track_caller]
pub(super) fn remove_gas_reservation<T>() -> (ProgramId, ReservationId)
where
    T: Config,
    T::AccountId: Origin,
{
    use binaries::demo_reserve_gas::{InitAction, WASM_BINARY_OPT};

    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY_OPT), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        WASM_BINARY_OPT.to_vec(),
        salt,
        InitAction::Normal(vec![(50_000, 100)]).encode(),
        10_000_000_000,
        0u32.into(),
        false,
    )
    .expect("submit program failed");

    Gear::<T>::process_queue(Default::default());

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
    use binaries::demo_constructor::{Call, Calls, Scheme, WASM_BINARY_OPT};

    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY_OPT), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY_OPT.to_vec(),
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

    Gear::<T>::process_queue(Default::default());

    let task = TaskPoolOf::<T>::iter_prefix_keys(Gear::<T>::block_number() + delay.into())
        .next()
        .unwrap();

    match task {
        ScheduledTask::SendDispatch(message_id) => message_id,
        _ => unreachable!(),
    }
}

#[track_caller]
pub(super) fn wake_message<T>() -> (ProgramId, MessageId)
where
    T: Config,
    T::AccountId: Origin,
{
    use binaries::demo_waiter::{Command, WaitSubcommand, WASM_BINARY_OPT};

    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY_OPT), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY_OPT.to_vec(),
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

    Gear::<T>::process_queue(Default::default());

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
pub(super) fn remove_from_waitlist<T>() -> (ProgramId, MessageId)
where
    T: Config,
    T::AccountId: Origin,
{
    use binaries::demo_waiter::{Command, WaitSubcommand, WASM_BINARY_OPT};

    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 200_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY_OPT), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller.clone()).into(),
        WASM_BINARY_OPT.to_vec(),
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

    Gear::<T>::process_queue(Default::default());

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
pub(super) fn remove_from_mailbox<T>() -> (ProgramId, MessageId)
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
