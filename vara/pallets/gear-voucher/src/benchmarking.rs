// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
use common::{Origin, benchmarking};
use frame_benchmarking::benchmarks;
use frame_support::traits::Currency;
use frame_system::{RawOrigin, pallet_prelude::BlockNumberFor};
use sp_runtime::traits::{One, UniqueSaturatedInto, Zero};

pub(crate) type CurrencyOf<T> = <T as Config>::Currency;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    issue {
        // Origin account.
        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Programs set.
        let set = (0..<<T as Config>::MaxProgramsAmount as Get<u8>>::get() as u32)
            .map(|i| benchmarking::account::<T::AccountId>("program", 0, i).cast())
            .collect();

        // Allow uploading codes.
        let code_uploading = true;

        // Voucher validity.
        let validity = <<T as Config>::MinDuration as Get<BlockNumberFor<T>>>::get();

    }: _(RawOrigin::Signed(origin.clone()), spender.clone(), balance, Some(set), code_uploading, validity)
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
        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Forbid uploading codes.
        let code_uploading = false;

        // Voucher validity.
        let validity = <<T as Config>::MinDuration as Get<BlockNumberFor<T>>>::get();

        // Issue voucher.
        assert!(Pallet::<T>::issue(RawOrigin::Signed(origin.clone()).into(), spender.clone(), balance, None, code_uploading, validity).is_ok());
        let (_, voucher_id, _) = Vouchers::<T>::iter().next().expect("Couldn't find voucher");

        frame_system::Pallet::<T>::set_block_number(frame_system::Pallet::<T>::block_number() + validity + One::one());
    }: _(RawOrigin::Signed(origin.clone()), spender.clone(), voucher_id)
    verify {
        assert!(CurrencyOf::<T>::free_balance(&voucher_id.cast::<T::AccountId>()).is_zero());
    }

    update {
        // Origin account.
        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Programs initial set.
        let amount = <<T as Config>::MaxProgramsAmount as Get<u8>>::get() as u32 / 2;
        let set = (0..amount)
            .map(|i| benchmarking::account::<T::AccountId>("program", 0, i).cast())
            .collect();

        // Forbid uploading codes.
        let code_uploading = false;

        // Voucher validity.
        let validity = <<T as Config>::MinDuration as Get<BlockNumberFor<T>>>::get();

        // Issue voucher.
        assert!(Pallet::<T>::issue(RawOrigin::Signed(origin.clone()).into(), spender.clone(), balance, Some(set), code_uploading, validity).is_ok());
        let (_, voucher_id, _) = Vouchers::<T>::iter().next().expect("Couldn't find voucher");

        // New owner account.
        let move_ownership = Some(benchmarking::account::<T::AccountId>("new_origin", 0, 0));

        // Balance top up.
        let balance_top_up = Some(balance);

        // Append programs set.
        let append_programs_set = (amount..<<T as Config>::MaxProgramsAmount as Get<u8>>::get() as u32)
            .map(|i| benchmarking::account::<T::AccountId>("program", 0, i).cast())
            .collect();
        let append_programs = Some(Some(append_programs_set));

        // Allow uploading codes.
        let code_uploading = Some(true);

        // prolong duration.
        let prolong_duration = Some(validity);
    }: _(RawOrigin::Signed(origin.clone()), spender.clone(), voucher_id, move_ownership, balance_top_up, append_programs, code_uploading, prolong_duration)
    verify {
        let voucher_info = Vouchers::<T>::get(spender, voucher_id).expect("Must be");
        assert_eq!(voucher_info.programs.map(|v| v.len()), Some(<<T as Config>::MaxProgramsAmount as Get<u8>>::get() as usize));
        assert_eq!(CurrencyOf::<T>::free_balance(&voucher_id.cast::<T::AccountId>()), balance * 2u128.unique_saturated_into());
    }

    decline {
        // Origin account.
        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            100_000_000_000_000_u128.unique_saturated_into()
        );

        // Spender account.
        let spender = benchmarking::account::<T::AccountId>("spender", 0, 1);

        // Voucher balance.
        let balance = 10_000_000_000_000_u128.unique_saturated_into();

        // Forbid uploading codes.
        let code_uploading = false;

        // Voucher validity.
        let validity = <<T as Config>::MinDuration as Get<BlockNumberFor<T>>>::get();

        // Issue voucher.
        assert!(Pallet::<T>::issue(RawOrigin::Signed(origin.clone()).into(), spender.clone(), balance, None, code_uploading, validity).is_ok());
        let (_, voucher_id, _) = Vouchers::<T>::iter().next().expect("Couldn't find voucher");
    }: _(RawOrigin::Signed(spender.clone()), voucher_id)
    verify {
        let voucher_info = Vouchers::<T>::get(spender, voucher_id).expect("Must be");
        assert_eq!(voucher_info.expiry, frame_system::Pallet::<T>::block_number());
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
