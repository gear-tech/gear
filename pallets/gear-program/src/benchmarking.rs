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

use super::*;
use common::{benchmarking, Origin};
use sp_runtime::traits::UniqueSaturatedInto;

#[allow(unused)]
use crate::Pallet as GearProgram;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use frame_system::RawOrigin;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    resume_program {
        let q in 1 .. 256;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm(q).unwrap();

        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).into_origin();
        benchmarking::set_program(program_id, code, q, 0u64);

        let memory_pages = common::get_program_pages(program_id, (0..q).collect()).unwrap();

        crate::Pallet::<T>::pause_program(program_id).unwrap();
    }: _(RawOrigin::Signed(caller), program_id, memory_pages, Default::default(), 10_000u32.into())
    verify {
        assert!(crate::Pallet::<T>::program_exists(program_id));
        assert!(!crate::Pallet::<T>::program_paused(program_id));
    }
}

impl_benchmark_test_suite!(GearProgram, crate::mock::new_test_ext(), crate::mock::Test);
