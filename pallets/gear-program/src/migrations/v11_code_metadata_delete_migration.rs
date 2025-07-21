// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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
    sp_runtime::{
        TryRuntimeError,
        codec::{Decode, Encode},
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 10;
const MIGRATE_TO_VERSION: u16 = 11;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 13;

pub struct MigrateRemoveCodeMetadata<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateRemoveCodeMetadata<T> {
    fn on_runtime_upgrade() -> Weight {
        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);

        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::in_code_storage_version();

            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);

            log::info!(
                "🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}."
            );

            let mut counter = 0;

            let mut removal_result = v10::MetadataStorage::<T>::clear(u32::MAX, None);
            // MultiRemovalResults contains two fields on which we calculate weight:
            // - loops: how many iterations of loop were performed, each requiring read
            // - backend: number of elements removed from database, corresponds to write.

            weight = weight.saturating_add(
                T::DbWeight::get()
                    .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
            );
            counter += removal_result.backend;

            while let Some(cursor) = removal_result.maybe_cursor.take() {
                removal_result = v10::MetadataStorage::<T>::clear(u32::MAX, Some(&cursor));
                weight = weight.saturating_add(
                    T::DbWeight::get()
                        .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
                );
                counter += removal_result.backend;
            }

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {counter} entries were cleared");
        } else {
            log::info!(
                "🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}"
            );
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(v10::MetadataStorage::<T>::iter().count() as u64)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        ensure!(
            v10::MetadataStorage::<T>::iter().count() == 0,
            "Metadata storage is not empty after upgrade"
        );

        Option::<u64>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?;

        Ok(())
    }
}

mod v10 {
    use primitive_types::H256;
    use sp_runtime::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
    };
    use sp_std::prelude::*;

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct CodeMetadata {
        pub author: H256,
        #[codec(compact)]
        pub block_number: u32,
    }

    use crate::{Config, Pallet};
    use frame_support::Identity;
    use gear_core::ids::CodeId;

    #[frame_support::storage_alias]
    pub type MetadataStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeId, CodeMetadata>;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;
    use sp_runtime::traits::Zero;

    #[test]
    fn v11_metadata_into_attribution_migration_works() {
        let _ = tracing_subscriber::fmt::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();

            let state = MigrateRemoveCodeMetadata::<Test>::pre_upgrade().unwrap();
            let w = MigrateRemoveCodeMetadata::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateRemoveCodeMetadata::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
