// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{Config, Pallet};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{codec::Encode, TryRuntimeError},
    sp_std::vec::Vec,
};

pub struct RemovePausedProgramStorageMigration<T: Config>(PhantomData<T>);

const MIGRATE_FROM_VERSION: u16 = 6;
const MIGRATE_TO_VERSION: u16 = 7;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 9;

impl<T: Config> OnRuntimeUpgrade for RemovePausedProgramStorageMigration<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();

        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            let mut counter = 0;

            let mut removal_result = onchain::PausedProgramStorage::<T>::clear(u32::MAX, None);
            // MultiRemovalResults contains two fields on which we calculate weight:
            // - loops: how many iterations of loop were performed, each requiring read
            // - backend: number of elements removed from database, corresponds to a write.

            weight = weight.saturating_add(
                T::DbWeight::get()
                    .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
            );
            counter += removal_result.backend;

            while let Some(cursor) = removal_result.maybe_cursor.take() {
                removal_result = onchain::PausedProgramStorage::<T>::clear(u32::MAX, Some(&cursor));
                weight = weight.saturating_add(
                    T::DbWeight::get()
                        .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
                );
                counter += removal_result.backend;
            }

            let mut removal_result = onchain::ResumeSessions::<T>::clear(u32::MAX, None);

            weight = weight.saturating_add(
                T::DbWeight::get()
                    .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
            );
            counter += removal_result.backend;

            while let Some(cursor) = removal_result.maybe_cursor.take() {
                removal_result = onchain::ResumeSessions::<T>::clear(u32::MAX, Some(&cursor));
                weight = weight.saturating_add(
                    T::DbWeight::get()
                        .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
                );
                counter += removal_result.backend;
            }

            onchain::ResumeSessionNonce::<T>::kill();
            // killing a storage: one write
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {counter} entries were cleared");
        } else {
            log::info!("🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(0)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
        ensure!(
            onchain::PausedProgramStorage::<T>::iter().count() == 0,
            "Paused program storage is not empty after upgrade"
        );

        ensure!(
            onchain::ResumeSessions::<T>::iter().count() == 0,
            "Resume sessions storage is not empty after upgrade"
        );

        ensure!(
            !onchain::ResumeSessionNonce::<T>::exists(),
            "Resume session nonce value was not deleted"
        );
        Ok(())
    }
}

mod onchain {
    use super::*;
    use alloc::collections::BTreeSet;
    use frame_support::{
        pallet_prelude::{StorageMap, StorageValue},
        traits::StorageInstance,
        Identity,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{
        ids::{CodeId, ProgramId},
        pages::{GearPage, WasmPage},
    };
    use primitive_types::H256;
    use sp_runtime::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
    };

    pub type PausedProgramStorage<T> = StorageMap<
        _GeneratedPrefixForPausedProgramStorage<T>,
        Identity,
        ProgramId,
        (BlockNumberFor<T>, H256),
    >;

    #[doc(hidden)]
    pub struct _GeneratedPrefixForPausedProgramStorage<T>(PhantomData<(T,)>);

    impl<T: Config> StorageInstance for _GeneratedPrefixForPausedProgramStorage<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<crate::Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "PausedProgramStorage";
    }

    pub type ResumeSessionNonce<T> = StorageValue<_GeneratedPrefixForResumeSessionNonce<T>, u32>;

    #[doc(hidden)]
    pub struct _GeneratedPrefixForResumeSessionNonce<T>(PhantomData<(T,)>);

    impl<T: Config> StorageInstance for _GeneratedPrefixForResumeSessionNonce<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<crate::Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }

        const STORAGE_PREFIX: &'static str = "ResumeSessionNonce";
    }

    #[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct ResumeSession<AccountId, BlockNumber> {
        pub(crate) page_count: u32,
        pub(crate) user: AccountId,
        pub(crate) program_id: ProgramId,
        pub(crate) allocations: BTreeSet<WasmPage>,
        pub(crate) pages_with_data: BTreeSet<GearPage>,
        pub(crate) code_hash: CodeId,
        pub(crate) end_block: BlockNumber,
    }

    pub type ResumeSessions<T> = StorageMap<
        _GeneratedPrefixForResumeSessions<T>,
        Identity,
        u32,
        ResumeSession<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>,
    >;

    #[doc(hidden)]
    pub struct _GeneratedPrefixForResumeSessions<T>(PhantomData<(T,)>);

    impl<T: Config> StorageInstance for _GeneratedPrefixForResumeSessions<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<crate::Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "ResumeSessions";
    }
}
