// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#[allow(unused)]
use crate::Pallet as Airdrop;
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

    transfer {
        let q in 1 .. 256;

        let source: T::AccountId = benchmarking::account("source", 0, 0);
        <T as pallet_gear::Config>::Currency::deposit_creating(&source, (1u128 << 60).unique_saturated_into());
        let recipient: T::AccountId = benchmarking::account("recipient", 0, 0);
        // Keeping in mind the existential deposit
        let amount = 100_000_u128.saturating_add(10_u128.saturating_mul(q.into()));

    }: _(RawOrigin::Root, source, recipient.clone(), amount.unique_saturated_into())
    verify {
        assert_eq!(pallet_balances::Pallet::<T>::total_balance(&recipient), amount.unique_saturated_into());
    }
}

impl_benchmark_test_suite!(Airdrop, crate::mock::new_test_ext(), crate::mock::Test,);
