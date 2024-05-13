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

const MIGRATE_FROM_VERSION: u16 = 5;
const MIGRATE_TO_VERSION: u16 = 6;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 6;

pub struct RemovePausedStorageMigration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for RemovePausedStorageMigration<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();

        let mut weight = T::DbWeight::get().reads(1);

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            if paused_program_storage::PausedProgramStorage::<T>::count() != 0 {
                log::error!("❌ Migration is not allowed for non-empty paused program storage");
                return weight;
            }
            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage.");
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

            ensure!(
                paused_program_storage::PausedProgramStorage::<T>::count() == 0,
                "Current paused program storage is not empty, not allowed for migration"
            );

            Some(0)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
        Ok(())
    }
}

mod paused_program_storage {
    use super::*;
    use frame_support::{
        pallet_prelude::CountedStorageMap, storage::types::CountedStorageMapInstance,
        traits::StorageInstance, Identity,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::ids::ProgramId;
    use primitive_types::H256;

    pub type PausedProgramStorage<T> = CountedStorageMap<
        _GeneratedPrefixForPausedProgramStorage<T>,
        Identity,
        ProgramId,
        (BlockNumberFor<T>, H256),
    >;

    #[doc(hidden)]
    pub struct _GeneratedPrefixForPausedProgramStorage<T>(PhantomData<(T,)>);

    #[doc(hidden)]
    pub struct _GeneratedCounterPrefixForPausedProgramStorage<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for _GeneratedCounterPrefixForPausedProgramStorage<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<crate::Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "CounterForPausedProgramStorage";
    }

    impl<T: Config> StorageInstance for _GeneratedPrefixForPausedProgramStorage<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<crate::Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "PausedProgramStorage";
    }

    impl<T: Config> CountedStorageMapInstance for _GeneratedPrefixForPausedProgramStorage<T> {
        type CounterPrefix = _GeneratedCounterPrefixForPausedProgramStorage<T>;
    }
}
