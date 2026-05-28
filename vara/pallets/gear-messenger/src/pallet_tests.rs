// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
