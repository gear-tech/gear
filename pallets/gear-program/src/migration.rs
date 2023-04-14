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

use crate::{Config, Pallet, ProgramStorage};
use common::Program;
use frame_support::{traits::StorageVersion, weights::Weight};
use sp_runtime::Saturating;

pub const FREE_PERIOD: u32 = 1_000;

const VERSION_1: StorageVersion = StorageVersion::new(1);

/// Wrapper for all migrations of this pallet, based on `StorageVersion`.
pub fn migrate<T: Config>() -> Weight {
    let version = StorageVersion::get::<Pallet<T>>();
    let weight: Weight = Weight::zero();

    if version == VERSION_1 {
        ProgramStorage::<T>::translate_values(
            |(program, block_number): (Program, <T as frame_system::Config>::BlockNumber)| {
                Some(common::program_storage::Item {
                    program,
                    block_number: block_number.saturating_add(FREE_PERIOD.into()),
                })
            },
        );

        super::pallet::PROGRAM_STORAGE_VERSION.put::<Pallet<T>>();
    }

    weight
}
