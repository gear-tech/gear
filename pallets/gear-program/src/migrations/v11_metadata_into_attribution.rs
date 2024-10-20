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

use crate::{CodeAttributionStorage, Config, Pallet};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use sp_std::marker::PhantomData;

use gear_core::code::CodeAttribution;
#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 10;
const MIGRATE_TO_VERSION: u16 = 11;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 13;

pub struct MigrateMetadataIntoAttribution<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateMetadataIntoAttribution<T> {
    fn on_runtime_upgrade() -> Weight {
        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);
        let mut counter = 0;

        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();

            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);

            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            v10::MetadataStorage::<T>::drain().for_each(|(code_id, metadata)| {
                // 1 read for metadata, 1 write for attribution
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                let attribution = CodeAttribution::new(metadata.author, metadata.block_number);
                CodeAttributionStorage::<T>::insert(code_id, attribution);

                counter += 1;
            });

            v10::MetadataStorageNonce::<T>::kill();
            // killing a storage: one write
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {counter} codes have been migrated");
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

            Some(v10::MetadataStorage::<T>::iter().count() as u64)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if let Some(old_count) = Option::<u64>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            let count = CodeAttributionStorage::<T>::iter_keys().count() as u64;
            ensure!(old_count == count, "incorrect count of elements");
        }

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
    use frame_support::{
        storage::types::{StorageMap, StorageValue},
        traits::{PalletInfo, StorageInstance},
        Identity,
    };
    use gear_core::ids::CodeId;
    use sp_std::marker::PhantomData;

    pub type MetadataStorageNonce<T> = StorageValue<MetadataStoragePrefix<T>, u32>;

    pub struct MetadataStoragePrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for MetadataStoragePrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }

        const STORAGE_PREFIX: &'static str = "MetadataStorage";
    }

    pub type MetadataStorage<T> =
        StorageMap<MetadataStoragePrefix<T>, Identity, CodeId, CodeMetadata>;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;
    use gear_core::ids::CodeId;
    use primitive_types::H256;
    use sp_runtime::traits::Zero;

    #[test]
    fn v11_metadata_into_attribution_migration_works() {
        let _ = env_logger::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();

            // add old code metadata
            let code_id = CodeId::from(1u64);
            let code_metadata = v10::CodeMetadata {
                author: H256::from([1u8; 32]),
                block_number: 1,
            };

            v10::MetadataStorage::<Test>::insert(code_id, code_metadata.clone());

            let state = MigrateMetadataIntoAttribution::<Test>::pre_upgrade().unwrap();
            let w = MigrateMetadataIntoAttribution::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateMetadataIntoAttribution::<Test>::post_upgrade(state).unwrap();

            let code_attribution = CodeAttributionStorage::<Test>::get(code_id).unwrap();

            assert_eq!(code_attribution.author, code_metadata.author);
            assert_eq!(code_attribution.block_number, code_metadata.block_number);

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
