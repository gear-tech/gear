// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use core::num::NonZeroU16;

pub struct PauseBatchCapacity;

impl sp_core::Get<NonZeroU16> for PauseBatchCapacity {
    fn get() -> NonZeroU16 {
        const PAUSE_BATCH_CAPACITY: NonZeroU16 = unsafe { NonZeroU16::new_unchecked(256) };

        PAUSE_BATCH_CAPACITY
    }
}

#[macro_export]
macro_rules! impl_config {
    ($( $tokens:tt )*) => {
        #[allow(dead_code)]
        type GearProgramConfigPauseBatchCapacity = $crate::pallet_tests::PauseBatchCapacity;

        mod pallet_tests_gear_program_config_impl {
            use super::*;

            $crate::impl_config_inner!($( $tokens )*);
        }
    };
}

#[macro_export]
macro_rules! impl_config_inner {
    ($runtime:ty$(,)?) => {
        impl pallet_gear_program::Config for $runtime {
            type Scheduler = GearScheduler;
            type CurrentBlockNumber = ();
            type PauseBatchCapacity = GearProgramConfigPauseBatchCapacity;
        }
    };

    ($runtime:ty, PauseBatchCapacity = $paused_batch_capacity:ty $(, $( $rest:tt )*)?) => {
        type GearProgramConfigPauseBatchCapacity = $paused_batch_capacity;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };
}
