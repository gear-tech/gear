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

#[macro_export]
macro_rules! impl_config {
    ($( $tokens:tt )*) => {
        #[allow(dead_code)]
        type GearConfigDebugInfo = ();
        #[allow(dead_code)]
        type GearConfigVoucher = ();
        #[allow(dead_code)]
        type GearConfigProgramRentEnabled = ConstBool<true>;
        #[allow(dead_code)]
        type GearConfigSchedule = ();

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
            type PerformanceMultiplier = PerformanceMultiplier;
            type DebugInfo = GearConfigDebugInfo;
            type CodeStorage = GearProgram;
            type ProgramStorage = GearProgram;
            type MailboxThreshold = ConstU64<3_000>;
            type ReservationsLimit = ConstU64<256>;
            type Messenger = GearMessenger;
            type GasProvider = GearGas;
            type BlockLimiter = GearGas;
            type Scheduler = GearScheduler;
            type QueueRunner = Gear;
            type Voucher = GearConfigVoucher;
            type ProgramRentFreePeriod = RentFreePeriod;
            type ProgramResumeMinimalRentPeriod = ResumeMinimalPeriod;
            type ProgramRentCostPerBlock = RentCostPerBlock;
            type ProgramResumeSessionDuration = ResumeSessionDuration;
            type ProgramRentEnabled = GearConfigProgramRentEnabled;
            type ProgramRentDisabledDelta = RentFreePeriod;
        }
    };

    ($runtime:ty, Schedule = $schedule:ty $(, $( $rest:tt )*)?) => {
        type GearConfigSchedule = $schedule;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, Voucher = $voucher:ty $(, $( $rest:tt )*)?) => {
        type GearConfigVoucher = $voucher;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, DebugInfo = $debug_info:ty $(, $( $rest:tt )*)?) => {
        type GearConfigDebugInfo = $debug_info;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };

    ($runtime:ty, ProgramRentEnabled = $program_rent_enabled:ty $(, $( $rest:tt )*)?) => {
        type GearConfigProgramRentEnabled = $program_rent_enabled;

        $crate::impl_config_inner!($runtime, $($( $rest )*)?);
    };
}
