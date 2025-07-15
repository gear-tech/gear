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
        type GearConfigVoucher = ();

        #[allow(dead_code)]
        type GearConfigSchedule = ();
        #[allow(dead_code)]
        type GearConfigBuiltinDispatcherFactory = ();
        #[allow(dead_code)]
        type GearRentPoolId = ();

        mod pallet_tests_gear_config_impl {
            use super::*;

            $crate::impl_config_inner!($( $tokens )*);
        }
    };
}

#[macro_export]
macro_rules! impl_config_inner {
    ($runtime:ty$(,)?) => {
        impl pallet_gear::Config for $runtime {
            type RuntimeEvent = RuntimeEvent;
            type Randomness = TestRandomness<Self>;
            type WeightInfo = pallet_gear::weights::SubstrateWeight<Self>;
            type Schedule = GearConfigSchedule;
            type OutgoingLimit = OutgoingLimit;
            type OutgoingBytesLimit = OutgoingBytesLimit;
            type PerformanceMultiplier = PerformanceMultiplier;
            type CodeStorage = GearProgram;
            type ProgramStorage = GearProgram;
            type MailboxThreshold = ConstU64<3_000>;
            type ReservationsLimit = ConstU64<256>;
            type Messenger = GearMessenger;
            type GasProvider = GearGas;
            type BlockLimiter = GearGas;
            type Scheduler = GearScheduler;
            type QueueRunner = Gear;
            type BuiltinDispatcherFactory = GearConfigBuiltinDispatcherFactory;
            type RentPoolId = GearRentPoolId;
        }
    };

    ($runtime:ty, Schedule = $schedule:ty $(, $( $rest:tt )*)?) => {
        type GearConfigSchedule = $schedule;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, BuiltinDispatcherFactory = $builtin_dispatcher_factory:ty $(, $( $rest:tt )*)?) => {
        type GearConfigBuiltinDispatcherFactory = $builtin_dispatcher_factory;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, RentPoolId = $rent_pool_id:ty $(, $( $rest:tt )*)?) => {
        type GearRentPoolId = $rent_pool_id;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };
}
