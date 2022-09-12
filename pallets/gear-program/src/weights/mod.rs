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

use frame_support::weights::{constants::RocksDbWeight, Weight};

mod pallet_gear_program;
pub use pallet_gear_program::SubstrateWeight;

/// Weight functions for pallet_gear_program.
pub trait WeightInfo {
    fn resume_program(q: u32) -> Weight;
}

// For backwards compatibility and tests
const SUBMIT_WEIGHT_PER_BYTE: u64 = 1_000_000;

impl WeightInfo for () {
    fn resume_program(q: u32) -> Weight {
        (0u64)
            .saturating_add(RocksDbWeight::get().writes(4u64))
            .saturating_add(SUBMIT_WEIGHT_PER_BYTE.saturating_mul(q as Weight))
    }
}
