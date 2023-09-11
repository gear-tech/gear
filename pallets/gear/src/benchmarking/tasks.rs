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

#[track_caller]
pub(super) fn send_user_message_prepare<T>(delay: u32)
where
    T: Config,
    T::AccountId: Origin,
{
    use demo_delayed_sender::WASM_BINARY;

    let caller = benchmarking::account("caller", 0, 0);
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
    )
    .expect("submit program failed");

    Gear::<T>::process_queue(Default::default());
}

#[track_caller]
pub(super) fn send_user_message_common<T>() -> MessageId
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
pub(super) fn pause_program_prepare<T: Config>(c: u32, code: Vec<u8>) -> ProgramId
where
    T::AccountId: Origin,
{
    let caller = benchmarking::account("caller", 0, 0);
    CurrencyOf::<T>::deposit_creating(&caller, 400_000_000_000_000u128.unique_saturated_into());

    init_block::<T>(None);

    let salt = vec![];
    let program_id = ProgramId::generate(CodeId::generate(&code), &salt);
    Gear::<T>::upload_program(
        RawOrigin::Signed(caller).into(),
        code,
        salt,
        b"init_payload".to_vec(),
        10_000_000_000,
        0u32.into(),
    )
    .expect("submit program failed");

    Gear::<T>::process_queue(Default::default());

    let memory_page = {
        let mut page = PageBuf::new_zeroed();
        page[0] = 1;

        page
    };

    for i in 0..c {
        ProgramStorageOf::<T>::set_program_page_data(
            program_id,
            GearPage::from(i as u16),
            memory_page.clone(),
        );
    }

    ProgramStorageOf::<T>::update_active_program(program_id, |program| {
        program.pages_with_data = BTreeSet::from_iter((0..c).map(|i| GearPage::from(i as u16)));

        let wasm_pages = (c as usize * GEAR_PAGE_SIZE) / WASM_PAGE_SIZE;
        program.allocations =
            BTreeSet::from_iter((0..wasm_pages).map(|i| WasmPage::from(i as u16)));
    })
    .expect("program should exist");

    program_id
}
