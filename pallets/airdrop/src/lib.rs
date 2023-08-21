// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    traits::{Currency, ExistenceRequirement, VestingSchedule},
};
pub use pallet::*;
use sp_runtime::traits::{Convert, Saturating};
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

pub(crate) type VestingBalanceOf<T> = <<T as pallet_vesting::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + pallet_gear::Config
        + pallet_balances::Config
        + pallet_vesting::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// To modify/remove vesting schedule
        type VestingSchedule: VestingSchedule<
            Self::AccountId,
            Currency = <Self as pallet_vesting::Config>::Currency,
            Moment = Self::BlockNumber,
        >;
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
        VestingScheduleRemoved {
            who: T::AccountId,
            schedule_index: u32,
        },
    }

    /// Error for the airdrop pallet.
    #[pallet::error]
    pub enum Error<T> {
        /// Amount to being transferred is bigger than vested.
        AmountBigger,
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
        #[pallet::weight(<T as Config>::WeightInfo::transfer(1))]
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

        /// Remove vesting for `source` account and transfer tokens to `dest` account.
        ///
        /// The origin must be the root.
        ///
        /// Parameters:
        /// - `source`: the account with vesting running,
        /// - `dest`: the beneficiary account,
        /// - `schedule_index`: the index of `VestingInfo` for source account.
        /// - `amount`: the amount to be unlocked and transfered from `VestingInfo`.
        ///
        /// Emits the following events:
        /// - `VestingScheduleRemoved{ who, schedule_index }`
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer_vested(1))]
        pub fn transfer_vested(
            origin: OriginFor<T>,
            source: T::AccountId,
            dest: T::AccountId,
            schedule_index: u32,
            amount: Option<VestingBalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            let schedules = pallet_vesting::Pallet::<T>::vesting(&source)
                .ok_or(pallet_vesting::Error::<T>::NotVesting)?;

            let schedule = schedules
                .get(schedule_index as usize)
                .ok_or(pallet_vesting::Error::<T>::ScheduleIndexOutOfBounds)?;

            T::VestingSchedule::remove_vesting_schedule(&source, schedule_index)?;

            Self::deposit_event(Event::VestingScheduleRemoved {
                who: source.clone(),
                schedule_index,
            });

            let amount = if let Some(amount) = amount {
                ensure!(amount <= schedule.locked(), Error::<T>::AmountBigger);
                let end_amount = schedule.locked().saturating_sub(amount);
                let end_block = schedule.ending_block_as_balance::<T::BlockNumberToBalance>();
                let start_block = T::BlockNumberToBalance::convert(schedule.starting_block());
                let per_block = end_amount / end_block.saturating_sub(start_block);

                T::VestingSchedule::can_add_vesting_schedule(
                    &source,
                    end_amount,
                    per_block,
                    schedule.starting_block(),
                )?;
                let res = T::VestingSchedule::add_vesting_schedule(
                    &source,
                    end_amount,
                    per_block,
                    schedule.starting_block(),
                );

                debug_assert!(
                    res.is_ok(),
                    "Failed to add a schedule when we had to succeed."
                );

                amount
            } else {
                schedule.locked()
            };

            <<T as pallet_vesting::Config>::Currency as Currency<_>>::transfer(
                &source,
                &dest,
                amount,
                ExistenceRequirement::AllowDeath,
            )?;

            // This extrinsic is not chargeable
            Ok(Pays::No.into())
        }
    }
}
