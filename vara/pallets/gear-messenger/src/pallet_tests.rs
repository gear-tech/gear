// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

#[macro_export]
macro_rules! impl_config {
    ($( $tokens:tt )*) => {
        #[allow(dead_code)]
        type GearMessengerCurrentBlockNumber = ();

        mod pallet_tests_gear_messenger_config_impl {
            use super::*;

            $crate::impl_config_inner!($( $tokens )*);
        }
    };
}

#[macro_export]
macro_rules! impl_config_inner {
    ($runtime:ty$(,)?) => {
        impl pallet_gear_messenger::Config for $runtime {
            type BlockLimiter = GearGas;
            type CurrentBlockNumber = GearMessengerCurrentBlockNumber;
        }
    };

    ($runtime:ty, CurrentBlockNumber = $current_block_number:ty $(, $( $rest:tt )*)?) => {
        type GearMessengerCurrentBlockNumber = $current_block_number;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };
}
