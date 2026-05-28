// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
