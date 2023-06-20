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

//! Benchmarks for the gear-voucher pallet

#[allow(unused)]
use crate::Pallet as GearVoucher;
use crate::*;
use common::{benchmarking, Origin};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use sp_runtime::traits::UniqueSaturatedInto;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    issue {
        let issuer = benchmarking::account::<T::AccountId>("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(
            &issuer,
            100_000_000_000_000_u128.unique_saturated_into()
        );
        let holder = benchmarking::account::<T::AccountId>("caller", 0, 1);
        let program_id = ProgramId::from_origin(
            benchmarking::account::<T::AccountId>("program", 0, 100).into_origin()
        );

        let holder_lookup = T::Lookup::unlookup(holder.clone());
    }: _(RawOrigin::Signed(issuer), holder_lookup, program_id, 10_000_000_000_000_u128.unique_saturated_into())
    verify {
        let voucher_account_id = GearVoucher::<T>::voucher_account_id(&holder, &program_id);
        assert_eq!(
            CurrencyOf::<T>::free_balance(&voucher_account_id),
            10_000_000_000_000_u128.unique_saturated_into(),
        );
    }
}

impl_benchmark_test_suite!(GearVoucher, crate::mock::new_test_ext(), crate::mock::Test,);
