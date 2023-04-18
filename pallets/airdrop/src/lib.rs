// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement},
};
pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub(crate) type BalanceOf<T> = <<T as pallet_gear::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_gear::Config + pallet_balances::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TokensDeposited {
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Transfer tokens from pre-funded `source` to `dest` account.
        ///
        /// The origin must be the root.
        ///
        /// Parameters:
        /// - `source`: the pre-funded account (i.e. root),
        /// - `dest`: the beneficiary account,
        /// - `amount`: the amount of tokens to be minted.
        ///
        /// Emits the following events:
        /// - `TokensDeposited{ dest, amount }`
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer())]
        pub fn transfer(
            origin: OriginFor<T>,
            source: T::AccountId,
            dest: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            <<T as pallet_gear::Config>::Currency as Currency<_>>::transfer(
                &source,
                &dest,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;
            Self::deposit_event(Event::TokensDeposited {
                account: dest,
                amount,
            });

            // This extrinsic is not chargeable
            Ok(Pays::No.into())
        }
    }
}
