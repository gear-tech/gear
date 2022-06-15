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

use common::CodeMetadata;
use gear_core::{code::InstrumentedCode, ids::CodeId};
pub use pallet::*;
use primitive_types::H256;
use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

use frame_support::{
    dispatch::DispatchResultWithPostInfo, traits::StorageVersion, weights::Weight,
};

mod code;
mod pause;
pub use pause::PauseError;

mod program;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod migration;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

/// The current storage version.
const PROGRAM_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        traits::{
            Currency, ExistenceRequirement, LockIdentifier, LockableCurrency, WithdrawReasons,
        },
    };
    use frame_system::pallet_prelude::*;
    use gear_core::memory::{vec_page_data_map_to_page_buf_map, PageNumber};
    use sp_runtime::traits::{UniqueSaturatedInto, Zero};
    use weights::WeightInfo;

    const LOCK_ID: LockIdentifier = *b"resume_p";

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        type Currency: LockableCurrency<Self::AccountId>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::storage_version(PROGRAM_STORAGE_VERSION)]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Program has been successfully resumed
        ProgramResumed(H256),
        /// Program has been paused
        ProgramPaused(H256),
    }

    #[pallet::error]
    pub enum Error<T> {
        PausedProgramNotFound,
        WrongMemoryPages,
        NotAllocatedPageWithData,
        ResumeProgramNotEnoughValue,
        WrongWaitList,
        InvalidPageData,
    }

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type CodeStorage<T: Config> = StorageMap<_, Identity, CodeId, InstrumentedCode>;

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type OriginalCodeStorage<T: Config> = StorageMap<_, Identity, CodeId, Vec<u8>>;

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type MetadataStorage<T: Config> = StorageMap<_, Identity, CodeId, CodeMetadata>;

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
        // TODO: unfortunatelly we cannot pass pages data in [PageBuf],
        // because polkadot-js api can not support this type.
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
        ///
        #[frame_support::transactional]
        #[pallet::weight(<T as Config>::WeightInfo::resume_program(memory_pages.values().map(|p| p.len() as u32).sum()))]
        pub fn resume_program(
            origin: OriginFor<T>,
            program_id: H256,
            memory_pages: BTreeMap<PageNumber, Vec<u8>>,
            wait_list: BTreeMap<H256, gear_core::message::StoredDispatch>,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let memory_pages = match vec_page_data_map_to_page_buf_map(memory_pages) {
                Ok(data) => data,
                Err(err) => {
                    log::debug!("resume program recieved wrong pages data: {}", err);
                    return Err(Error::<T>::InvalidPageData.into());
                }
            };

            let account = ensure_signed(origin)?;

            ensure!(!value.is_zero(), Error::<T>::ResumeProgramNotEnoughValue);

            Self::resume_program_impl(
                program_id,
                memory_pages,
                wait_list,
                <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
            )?;

            let program_account = &<T::AccountId as common::Origin>::from_origin(program_id);
            // The value movement `transfer` call respects existence requirements rules, so no need to check
            // value for being in the valid interval like it's done in `pallet_gear` calls.
            T::Currency::transfer(
                &account,
                program_account,
                value,
                ExistenceRequirement::AllowDeath,
            )?;

            // TODO: maybe it is sufficient just to reserve value? (#762)
            T::Currency::extend_lock(LOCK_ID, program_account, value, WithdrawReasons::FEE);

            Self::deposit_event(Event::ProgramResumed(program_id));

            Ok(().into())
        }
    }
}
