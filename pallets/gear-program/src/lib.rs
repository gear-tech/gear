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
pub use pause::PauseError;
pub use weights::WeightInfo;

mod pause;
mod program;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod migration;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    pub use frame_support::weights::Weight;

    pub(crate) type WaitlistOf<T> = <<T as Config>::Messenger as Messenger>::Waitlist;

    use super::*;
    use common::{storage::*, CodeMetadata, Origin as _};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        traits::{
            Currency, ExistenceRequirement, LockIdentifier, LockableCurrency, StorageVersion,
            WithdrawReasons,
        },
    };
    use frame_system::pallet_prelude::*;
    use gear_core::{
        code::InstrumentedCode,
        ids::{CodeId, MessageId, ProgramId},
        memory::{vec_page_data_map_to_page_buf_map, PageNumber},
        message::StoredDispatch,
    };
    use sp_runtime::{traits::Zero, DispatchError};
    use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

    const LOCK_ID: LockIdentifier = *b"resume_p";

    /// The current storage version.
    const PROGRAM_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        type Currency: LockableCurrency<Self::AccountId>;

        type Messenger: Messenger<
            BlockNumber = Self::BlockNumber,
            OutputError = DispatchError,
            WaitlistFirstKey = ProgramId,
            WaitlistSecondKey = MessageId,
            WaitlistedMessage = StoredDispatch,
        >;
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
        ProgramResumed(ProgramId),
        /// Program has been paused
        ProgramPaused(ProgramId),
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

    common::wrap_storage_map!(
        storage: CodeStorage,
        name: CodeStorageWrap,
        key: CodeId,
        value: InstrumentedCode
    );

    #[pallet::storage]
    pub(crate) type CodeLenStorage<T: Config> = StorageMap<_, Identity, CodeId, u32>;

    common::wrap_storage_map!(
        storage: CodeLenStorage,
        name: CodeLenStorageWrap,
        key: CodeId,
        value: u32
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type OriginalCodeStorage<T: Config> = StorageMap<_, Identity, CodeId, Vec<u8>>;

    common::wrap_storage_map!(
        storage: OriginalCodeStorage,
        name: OriginalCodeStorageWrap,
        key: CodeId,
        value: Vec<u8>
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type MetadataStorage<T: Config> = StorageMap<_, Identity, CodeId, CodeMetadata>;

    common::wrap_storage_map!(
        storage: MetadataStorage,
        name: MetadataStorageWrap,
        key: CodeId,
        value: CodeMetadata
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type PausedPrograms<T: Config> =
        StorageMap<_, Identity, ProgramId, pause::PausedProgram>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> common::CodeStorage for pallet::Pallet<T> {
        type InstrumentedCodeStorage = CodeStorageWrap<T>;
        type InstrumentedLenStorage = CodeLenStorageWrap<T>;
        type MetadataStorage = MetadataStorageWrap<T>;
        type OriginalCodeStorage = OriginalCodeStorageWrap<T>;
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: common::Origin,
    {
        // TODO: unfortunately we cannot pass pages data in [PageBuf],
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
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::resume_program(memory_pages.values().map(|p| p.len() as u32).sum()))]
        pub fn resume_program(
            origin: OriginFor<T>,
            program_id: ProgramId,
            memory_pages: BTreeMap<PageNumber, Vec<u8>>,
            wait_list: BTreeMap<MessageId, gear_core::message::StoredDispatch>,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let memory_pages = match vec_page_data_map_to_page_buf_map(memory_pages) {
                Ok(data) => data,
                Err(err) => {
                    log::debug!("resume program received wrong pages data: {}", err);
                    return Err(Error::<T>::InvalidPageData.into());
                }
            };

            let account = ensure_signed(origin)?;

            ensure!(!value.is_zero(), Error::<T>::ResumeProgramNotEnoughValue);

            Self::resume_program_impl(program_id, memory_pages, wait_list)?;

            // The value movement `transfer` call respects existence requirements rules, so no need to check
            // value for being in the valid interval like it's done in `pallet_gear` calls.
            let program_account =
                &<T::AccountId as common::Origin>::from_origin(program_id.into_origin());
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
