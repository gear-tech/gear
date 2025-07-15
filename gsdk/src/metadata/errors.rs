// This file is part of Gear.
//
// Copyright (C) 2022-2025 Gear Technologies Inc.
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

// TODO: refactor this after #2618
macro_rules! export_module_error {
    ($($path:ident)::* => $error:ident => $index:expr) => {
        pub use crate::metadata::runtime_types::$($path)::*::pallet::Error as $error;
    };
    ($($($path:ident)::* => $error:ident => $index:expr),*) => {
        $(
            export_module_error!($($path)::* => $error => $index);
        )*

        /// Common error type for runtime modules.
        #[derive(Debug)]
        pub enum ModuleError {
            $($error($error)),*,
            Unknown {
                pallet_index: u8,
                error: [u8; 4],
            }
        }

        impl Display for ModuleError {
             fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                 write!(f, "{:?}", self)
             }
        }

        impl std::error::Error for ModuleError {}

        impl From<subxt::error::ModuleError> for ModuleError {
            fn from(e: subxt::error::ModuleError) -> ModuleError {
                let mut error = [0; 4];
                error.copy_from_slice(&e.bytes()[1..]);

                match e.pallet_index() {
                     $($index => match $error::decode(&mut [e.error_index()].as_ref()) {
                         Ok(e) => ModuleError::$error(e),
                         Err(_) => ModuleError::Unknown {
                             pallet_index: e.pallet_index(),
                             error,
                         },
                     }),*,
                     // `pallet_fellowship_referenda => 19`
                     //
                     // shares the same error with
                     //
                     // `pallet_fellowship_collective => 18`
                     19 => {
                         let mb_error = RanckedCollective::decode(&mut error.as_ref());
                         match mb_error {
                             Ok(e) => ModuleError::RanckedCollective(e),
                             Err(_) => ModuleError::Unknown {
                                 pallet_index: e.pallet_index(),
                                 error,
                             },
                         }
                     },
                     _ => ModuleError::Unknown {
                         pallet_index: e.pallet_index(),
                         error,
                     }
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
    pallet_gear_staking_rewards => GearStakingRewards => 106
}
