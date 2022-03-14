// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

pub use pallet::*;
use primitive_types::H256;
use sp_std::prelude::*;
use sp_std::collections::btree_map::BTreeMap;

mod pause;
pub use pause::PauseError;

mod program;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::{Currency, ExistenceRequirement}};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{Zero, UniqueSaturatedInto};
    use weights::Info;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: weights::Info;

        type Currency: Currency<Self::AccountId>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Program has been successfully resumed
        ProgramResumed(H256),
    }

    #[pallet::error]
    pub enum Error<T> {
        ProgramNotFound,
        WrongMemoryPages,
        ResumeProgramNotEnoughValue,
    }

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type PausedPrograms<T: Config> = StorageMap<_, Identity, H256, pause::PausedProgram>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: common::Origin,
    {
        /// Resumes a previously paused program
        ///
        /// The origin must be Signed and the sender must have sufficient funds to
        /// transfer value to the program.
        ///
        /// Parameters:
        /// - `program_id`: id of the program to resume.
        /// - `memory_pages`: program memory before it was paused.
        /// - `value`: balance to be transferred to the program once it's been resumed.
        ///
        /// - `ProgramResumed(H256)` in the case of success.
        #[frame_support::transactional]
        #[pallet::weight(<T as Config>::WeightInfo::resume_program(memory_pages.values().map(|p| p.len() as u32).sum()))]
        pub fn resume_program(
            origin: OriginFor<T>,
            program_id: H256,
            memory_pages: BTreeMap<u32, Vec<u8>>,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let account = ensure_signed(origin)?;

            ensure!(!value.is_zero(), Error::<T>::ResumeProgramNotEnoughValue);

            Self::resume_program_impl(
                program_id,
                memory_pages,
                <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
            )?;

            T::Currency::transfer(
                &account,
                &<T::AccountId as common::Origin>::from_origin(program_id),
                value,
                ExistenceRequirement::AllowDeath,
            )?;

            Self::deposit_event(Event::ProgramResumed(program_id));

            Ok(().into())
        }
    }
}
