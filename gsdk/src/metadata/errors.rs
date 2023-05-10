// This file is part of Gear.
//
// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt::{Debug, Display};

/// Common error type for modules.
pub trait ModuleError: Debug {}

// TODO: apply `Display` with docs to all module errors in #2618
impl Display for dyn ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for dyn ModuleError {}

impl<T: Debug> ModuleError for T {}

macro_rules! export_module_error {
    ($($path:ident)::* => $error:ident) => {
        pub use crate::metadata::runtime_types::$($path)::*::pallet::Error as $error;
    };
    ($($($path:ident)::* => $error:ident),*) => {
        $(
            export_module_error!($($path)::* => $error);
        )*
    };
}

// # NOTE
//
// pallets that don't have `Error` type
//
// - pallet_transaction_payment
// - pallet_airdrop
export_module_error! {
    frame_system => System,
    pallet_grandpa => Grandpa,
    pallet_balances => Balances,
    pallet_vesting => Vesting,
    pallet_bags_list => BagsList,
    pallet_im_online => ImOnline,
    pallet_staking::pallet => Staking,
    pallet_session => Session,
    pallet_treasury => Treasury,
    pallet_conviction_voting => ConvictionVoting,
    pallet_referenda => Referenda,
    pallet_ranked_collective => FellowshipCollective,
    pallet_ranked_collective => FellowshipReferenda,
    pallet_whitelist => Whitelist,
    pallet_sudo => Sudo,
    pallet_scheduler => Scheduler,
    pallet_preimage => Preimage,
    pallet_identity => Identity,
    pallet_utility => Utility,
    pallet_gear => Gear,
    pallet_gear_staking_rewards => GearStakingRewards,
    pallet_gear_debug => GearDebug
}
