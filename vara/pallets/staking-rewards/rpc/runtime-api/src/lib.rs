// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet_gear_staking_rewards::InflationInfo;

sp_api::decl_runtime_apis! {
    pub trait GearStakingRewardsApi {
        /// Calculate token economics related data.
        fn inflation_info() -> InflationInfo;
    }
}
