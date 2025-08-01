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

//! Benchmarks for Pallet Gear Eth Bridge.

use crate::{Call, Config, CurrencyOf, Pallet};
use common::{Origin, benchmarking};
use frame_benchmarking::benchmarks;
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use sp_runtime::traits::{Get, UniqueSaturatedInto};
use sp_std::vec;

#[cfg(test)]
use crate::mock;

benchmarks! {
    where_clause { where T::AccountId: Origin }

    pause {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);

        // Initially pallet is paused so we need to unpause it first.
        assert!(Pallet::<T>::unpause(RawOrigin::Root.into()).is_ok());
    }: _(RawOrigin::Root)
    verify {
        assert!(crate::Paused::<T>::get());
    }

    unpause {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);
    }: _(RawOrigin::Root)
    verify {
        assert!(!crate::Paused::<T>::get());
    }

    set_fee {
        let fee = 4242424242424242u128.unique_saturated_into();
    } : _(RawOrigin::Root, fee)
    verify {
        assert_eq!(crate::TransportFee::<T>::get(), 4242424242424242u128.unique_saturated_into());
    }

    send_eth_message {
        // Initially pallet is uninitialized so we hack it for benchmarks.
        crate::Initialized::<T>::put(true);

        // Set fee to minimum balance for the benchmark.
        assert!(Pallet::<T>::set_fee(RawOrigin::Root.into(), CurrencyOf::<T>::minimum_balance()).is_ok());
        // Initially pallet is paused so we need to unpause it first.
        assert!(Pallet::<T>::unpause(RawOrigin::Root.into()).is_ok());

        let origin = benchmarking::account::<T::AccountId>("origin", 0, 0);
        let _ = crate::CurrencyOf::<T>::deposit_creating(&origin, CurrencyOf::<T>::minimum_balance());

        let destination = [42; 20].into();

        let payload = vec![42; T::MaxPayloadSize::get() as usize];
    }: _(RawOrigin::Signed(origin), destination, payload)
    verify {
        assert!(!crate::Queue::<T>::get().is_empty());
    }

    impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
