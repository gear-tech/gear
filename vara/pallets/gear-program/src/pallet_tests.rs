// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[macro_export]
macro_rules! impl_config {
    ($runtime:ty) => {
        impl pallet_gear_program::Config for $runtime {
            type Scheduler = GearScheduler;
            type CurrentBlockNumber = ();
        }
    };
}
