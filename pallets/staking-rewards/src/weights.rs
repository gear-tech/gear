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

//! Weights are borrowed from the pallet_balances benchmarking results since all the
//! dispatchables from this pallet have an exact counterpart in the pallet_balances

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

// (issue #2531)
#![allow(deprecated)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_treasury.
pub trait WeightInfo {
	fn refill() -> Weight;
	fn force_refill() -> Weight;
	fn withdraw() -> Weight;
}

/// Weights for pallet_treasury using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Weights borrowed from `vara_runtime::weights::pallet_balances::SubstrateWeight::transfer()`
	// Added another DB write for depositing an event
	fn refill() -> Weight {
		Weight::from_ref_time(55_241_000_u64)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(2_u64))
	}

	fn force_refill() -> Weight {
		// Same as `vara_runtime::weights::pallet_balances::SubstrateWeight::force_transfer()`
		// except for an additional DB write for depositing event
		Weight::from_ref_time(54_529_000_u64)
            .saturating_add(T::DbWeight::get().reads(2_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
	}

	fn withdraw() -> Weight {
		// Same as `force_refill()`
		Weight::from_ref_time(54_529_000_u64)
            .saturating_add(T::DbWeight::get().reads(2_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn refill() -> Weight {
		Weight::from_ref_time(55_241_000_u64)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn force_refill() -> Weight {
		Weight::from_ref_time(54_529_000_u64)
            .saturating_add(RocksDbWeight::get().reads(2_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn withdraw() -> Weight {
		Weight::from_ref_time(54_529_000_u64)
            .saturating_add(RocksDbWeight::get().reads(2_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
	}
}
