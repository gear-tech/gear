// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{benchmarking::tests::utils, BalanceOf, Config, CurrencyOf};
use alloc::{vec, vec::Vec};
use frame_support::traits::{Currency, Get};
use parity_scale_codec::Encode;
use sp_runtime::SaturatedConversion;

pub fn smoke<T: Config>() {
    const MAIN_KEY: &[u8] = b"MAIN_KEY";
    const MAIN_VALUE: &[u8] = b"MAIN_VALUE";

    const THREAD_KEY: &[u8] = b"THREAD_KEY";
    const THREAD_VALUE: &[u8] = b"THREAD_VALUE";

    #[cfg(feature = "std")]
    utils::init_logger();

    gear_runtime_interface::reinit_tasks(T::ProcessingTasksAmount::get());

    sp_io::storage::set(MAIN_KEY, MAIN_VALUE);

    let unsorted = vec![9, 7, 5, 3, 2, 1];
    let handle = gear_tasks::spawn(
        |mut payload| {
            let bank_address = <T as pallet_gear_bank::Config>::BankAddress::get();
            let balance = CurrencyOf::<T>::free_balance(&bank_address);

            assert_eq!(sp_io::storage::get(MAIN_KEY).as_deref(), Some(MAIN_VALUE));

            sp_io::storage::set(THREAD_KEY, THREAD_VALUE);

            payload.sort();
            (payload, balance).encode()
        },
        unsorted,
    );

    let payload = handle.join().unwrap();
    let (sorted, bank_balance): (Vec<u8>, BalanceOf<T>) =
        parity_scale_codec::Decode::decode(&mut &payload[..]).unwrap();
    assert_eq!(sorted, vec![1, 2, 3, 5, 7, 9]);
    assert_eq!(bank_balance, CurrencyOf::<T>::minimum_balance());

    assert_eq!(sp_io::storage::get(THREAD_KEY), None);

    log::info!("Bank balance: {}", bank_balance.saturated_into::<u128>());
}
