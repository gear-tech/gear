// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Benchmarks for Pallet Gear Voucher.

use crate::*;
use common::{benchmarking, Origin};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, Zero};
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use sp_runtime::traits::{One, UniqueSaturatedInto};

pub(crate) type CurrencyOf<T> = <T as Config>::Currency;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    issue {
        // Origin account.
        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Programs set.
        let set = (0..=<<T as Config>::MaxProgramsAmount as Get<u8>>::get() as u32)
            .map(|i| benchmarking::account::<T::AccountId>("program", 0, i).cast())
            .collect();

        // Voucher validity.
        let validity = 100u32.unique_saturated_into();

    }: _(RawOrigin::Signed(origin.clone()), spender.clone(), balance, Some(set), validity)
    verify {
        let (key_spender, voucher_id, voucher_info) = Vouchers::<T>::iter().next().expect("Couldn't find voucher");

        assert_eq!(key_spender, spender);
        assert_eq!(voucher_info.owner, origin);
        assert_eq!(
            CurrencyOf::<T>::free_balance(&voucher_id.cast::<T::AccountId>()),
            balance,
        );
    }

    revoke {
        // Origin account.
        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Voucher validity.
        let validity = 100u32.unique_saturated_into();

        // Issue voucher.
        assert!(Pallet::<T>::issue(RawOrigin::Signed(origin.clone()).into(), spender.clone(), balance, None, validity).is_ok());
        let (_, voucher_id, _) = Vouchers::<T>::iter().next().expect("Couldn't find voucher");

        frame_system::Pallet::<T>::set_block_number(frame_system::Pallet::<T>::block_number() + validity + One::one());
    }: _(RawOrigin::Signed(origin.clone()), spender.clone(), voucher_id)
    verify {
        assert!(CurrencyOf::<T>::free_balance(&voucher_id.cast::<T::AccountId>()).is_zero());
    }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
