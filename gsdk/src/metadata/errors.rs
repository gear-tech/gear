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
use subxt::ext::codec::Decode;

/// Common error type for modules.
pub trait ModuleError: Debug {
    /// Returns the pallet index of this error.
    fn pallet_index(&self) -> u8;
}

impl ModuleError for subxt::error::ModuleError {
    fn pallet_index(&self) -> u8 {
        self.error_data.pallet_index
    }
}

// TODO: apply `Display` with docs to all module errors in #2618
impl Display for dyn ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// TODO: refactor this after #2618
macro_rules! export_module_error {
    ($($path:ident)::* => $error:ident => $index:expr) => {
        pub use crate::metadata::runtime_types::$($path)::*::pallet::Error as $error;

        impl ModuleError for $error {
            fn pallet_index(&self) -> u8 {
                $index
            }
        }
    };
    ($($($path:ident)::* => $error:ident => $index:expr),*) => {
        $(
            export_module_error!($($path)::* => $error => $index);
        )*

        impl From<subxt::error::ModuleError> for Box<dyn ModuleError> {
            fn from(error: subxt::error::ModuleError) -> Box<dyn ModuleError> {
                match error.error_data.pallet_index {
                    $($index => {
                        let mb_error = $error::decode(&mut error.error_data.error.as_ref());
                        match mb_error {
                            Ok(e) => Box::new(e),
                            Err(_) => Box::new(error)
                        }
                    }),*,
                    // `pallet_fellowship_referenda => 19`
                    //
                    // shares the same error with
                    //
                    // `pallet_fellowship_collective => 18`
                    19 => {
                        let mb_error = RanckedCollective::decode(&mut error.error_data.error.as_ref());
                        match mb_error {
                            Ok(e) => Box::new(e),
                            Err(_) => Box::new(error)
                        }
                    },
                    _ => Box::new(error)
                }
            }
        }
    };
}

// Re-exports module errors from runtime types.
//
//
// # NOTE
//
// pallets that don't have `Error` type.
//
// - pallet_transaction_payment
// - pallet_airdrop
//
// pallets that share the same `errors::RankedCollective`
//
// - pallet_fellowship_collective => 18
// - pallet_fellowship_referenda => 19
export_module_error! {
    frame_system => System => 0,
    pallet_grandpa => Grandpa => 4,
    pallet_balances => Balances => 5,
    pallet_vesting => Vesting => 10,
    pallet_bags_list => BagsList => 11,
    pallet_im_online => ImOnline => 12,
    pallet_staking::pallet => Staking => 13,
    pallet_session => Session => 7,
    pallet_treasury => Treasury => 14,
    pallet_conviction_voting => ConvictionVoting => 16,
    pallet_referenda => Referenda => 17,
    pallet_ranked_collective => RanckedCollective => 18,
    pallet_whitelist => Whitelist => 21,
    pallet_sudo => Sudo => 99,
    pallet_scheduler => Scheduler => 22,
    pallet_preimage => Preimage => 23,
    pallet_identity => Identity => 24,
    pallet_utility => Utility => 8,
    pallet_gear => Gear => 104,
    pallet_gear_staking_rewards => GearStakingRewards => 106,
    pallet_gear_debug => GearDebug => 199
}
