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
#[allow(unused)]
use crate::Pallet as GearProgram;
use common::{benchmarking, Origin};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use gear_core::{
    ids::ProgramId,
    memory::{PageNumber, WasmPageNumber, to_page_iter},
};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::vec::Vec;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    resume_program {
        let q in 1 .. 128;
        let q = q as u16;
        let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm(q.into()).unwrap();

        let program_id = ProgramId::from_origin(benchmarking::account::<T::AccountId>("program", 0, 100).into_origin());
        benchmarking::set_program(program_id.into_origin(), code, q.into());

        let wasm_pages = (0.into()..q.into()).collect::<Vec<WasmPageNumber>>();
        let pages: Vec<PageNumber> = wasm_pages.iter().flat_map(|&p| to_page_iter(p)).collect();
        let memory_pages = common::get_program_data_for_pages(program_id.into_origin(), pages.iter()).unwrap().into_iter().map(|(page, data)| (page, data.into_vec())).collect();

        crate::Pallet::<T>::pause_program(program_id).unwrap();
    }: _(RawOrigin::Signed(caller), program_id, memory_pages, Default::default(), minimum_balance)
    verify {
        assert!(crate::Pallet::<T>::program_exists(program_id));
        assert!(!crate::Pallet::<T>::program_paused(program_id));
    }
}

impl_benchmark_test_suite!(GearProgram, crate::mock::new_test_ext(), crate::mock::Test);
