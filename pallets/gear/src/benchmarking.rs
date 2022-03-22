// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear pallet benchmarking

use super::*;
use common::{benchmarking, Origin};
use gear_core::identifiers::{CodeId, MessageId, ProgramId};
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::prelude::*;

#[allow(unused)]
use crate::Pallet as Gear;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use frame_system::RawOrigin;

const MIN_CODE_LEN: u32 = 128;
const MAX_CODE_LEN: u32 = 128 * 1024;
const MAX_PAYLOAD_LEN: u32 = 64 * 1024;
const MAX_PAGES: u32 = 512;

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    submit_code {
        let c in MIN_CODE_LEN .. MAX_CODE_LEN;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        let code = benchmarking::generate_wasm3(vec![0u8; (c - MIN_CODE_LEN) as usize]).unwrap();
        let code_hash: H256 = CodeId::generate(&code).into_origin();
    }: _(RawOrigin::Signed(caller), code)
    verify {
        assert!(common::code_exists(code_hash));
    }

    submit_program {
        let c in MIN_CODE_LEN .. MAX_CODE_LEN;
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm3(vec![0u8; (c - MIN_CODE_LEN) as usize]).unwrap();
        let salt = vec![255u8; 32];
        let payload = vec![1_u8; p as usize];
        // Using a non-zero `value` to count in the transfer, as well
        let value = 10_000_u32;
    }: _(RawOrigin::Signed(caller), code, salt, payload, 100_000_000_u64, value.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).into_origin();
        let code = benchmarking::generate_wasm2(16_i32).unwrap();
        benchmarking::set_program(program_id, code, 1_u32);
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).into_origin();
        let code = benchmarking::generate_wasm2(16_i32).unwrap();
        benchmarking::set_program(program_id, code, 1_u32);
        let original_message_id = benchmarking::account::<T::AccountId>("message", 0, 100).into_origin();
        Gear::<T>::insert_to_mailbox(
            caller.clone().into_origin(),
            gear_core::message::StoredMessage::new(
                MessageId::from_origin(original_message_id),
                ProgramId::from_origin(program_id),
                ProgramId::from_origin(caller.clone().into_origin()),
                Default::default(),
                0,
                None,
            )
        );
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), original_message_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    initial_allocation {
        let q in 1 .. MAX_PAGES;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm(q).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = crate::Pallet::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        crate::Pallet::<T>::process_queue();
    }
    verify {
        assert!(common::dequeue_dispatch().is_none());
    }

    alloc_in_handle {
        let q in 0 .. MAX_PAGES;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as Config>::Currency::deposit_creating(&caller, (1_u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm2(q as i32).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = crate::Pallet::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        crate::Pallet::<T>::process_queue();
    }
    verify {
        assert!(common::dequeue_dispatch().is_none());
    }
}

impl_benchmark_test_suite!(Gear, crate::mock::new_test_ext(), crate::mock::Test);
