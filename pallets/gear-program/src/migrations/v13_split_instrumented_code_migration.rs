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

use crate::{CodeMetadataStorage, Config, Pallet};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use gear_core::code::CodeMetadata;
use sp_std::marker::PhantomData;

use gear_core::code::InstrumentationStatus;
#[cfg(feature = "try-runtime")]
use {
    crate::InstrumentedCodeStorage,
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 12;
const MIGRATE_TO_VERSION: u16 = 13;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 13;

pub struct MigrateSplitInstrumentedCode<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateSplitInstrumentedCode<T> {
    fn on_runtime_upgrade() -> Weight {
        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);
        let mut counter = 0;

        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::in_code_storage_version();

            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);

            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            v12::CodeStorage::<T>::drain().for_each(|(code_id, instrumented_code)| {
                // 1 read for instrumented code and 1 write for code metadata, also 1 write to delete instrumented code from previous storage
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));

                // We skip writing instrumented code into storage, as it will be written in the next reinstrumentation
                let code_metadata = CodeMetadata::new(
                    instrumented_code.original_code_len,
                    instrumented_code.exports,
                    instrumented_code.static_pages,
                    instrumented_code.stack_end,
                    InstrumentationStatus::NotInstrumented,
                );

                CodeMetadataStorage::<T>::insert(code_id, code_metadata);

                counter += 1;
            });

            let mut removal_result = v12::CodeLenStorage::<T>::clear(u32::MAX, None);

            weight = weight.saturating_add(
                T::DbWeight::get()
                    .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
            );

            while let Some(cursor) = removal_result.maybe_cursor.take() {
                removal_result = v12::CodeLenStorage::<T>::clear(u32::MAX, Some(&cursor));
                weight = weight.saturating_add(
                    T::DbWeight::get()
                        .reads_writes(removal_result.loops as u64, removal_result.backend as u64),
                );
            }

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
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(v12::CodeStorage::<T>::iter().count() as u64)
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
            let count_instrumented_code = InstrumentedCodeStorage::<T>::iter_keys().count() as u64;
            let count_code_metadata = CodeMetadataStorage::<T>::iter_keys().count() as u64;

            ensure!(
                count_instrumented_code == 0 && count_code_metadata == old_count,
                "incorrect count of elements"
            );
        }

        Ok(())
    }
}

mod v12 {
    use gear_core::{
        code::InstantiatedSectionSizes,
        message::DispatchKind,
        pages::{WasmPage, WasmPagesAmount},
    };
    use sp_runtime::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
    };
    use sp_std::{collections::btree_set::BTreeSet, prelude::*};

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct InstrumentedCode {
        pub code: Vec<u8>,
        pub original_code_len: u32,
        pub exports: BTreeSet<DispatchKind>,
        pub static_pages: WasmPagesAmount,
        pub stack_end: Option<WasmPage>,
        pub instantiated_section_sizes: InstantiatedSectionSizes,
        pub version: u32,
    }

    use crate::{Config, Pallet};
    use frame_support::Identity;
    use gear_core::primitives::CodeId;

    #[frame_support::storage_alias]
    pub type CodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeId, InstrumentedCode>;

    #[frame_support::storage_alias]
    pub type CodeLenStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeId, u32>;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use common::CodeId;
    use frame_support::traits::StorageVersion;
    use gear_core::{code::InstantiatedSectionSizes, pages::WasmPagesAmount};
    use sp_runtime::traits::Zero;

    #[test]
    fn v13_split_instrumented_code_migration_works() {
        let _ = tracing_subscriber::fmt::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();

            let code_id = CodeId::from(1u64);

            let section_sizes = InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0);

            let instrumented_code = v12::InstrumentedCode {
                code: vec![1u8; 32],
                original_code_len: 32,
                exports: Default::default(),
                static_pages: WasmPagesAmount::from(1u16),
                stack_end: None,
                instantiated_section_sizes: section_sizes,
                version: 1,
            };

            v12::CodeStorage::<Test>::insert(code_id, instrumented_code.clone());

            let state = MigrateSplitInstrumentedCode::<Test>::pre_upgrade().unwrap();
            let w = MigrateSplitInstrumentedCode::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateSplitInstrumentedCode::<Test>::post_upgrade(state).unwrap();

            let code_metadata = CodeMetadataStorage::<Test>::get(code_id).unwrap();

            assert_eq!(
                code_metadata.original_code_len(),
                instrumented_code.original_code_len
            );
            assert_eq!(code_metadata.exports(), &instrumented_code.exports);
            assert_eq!(code_metadata.static_pages(), instrumented_code.static_pages);
            assert_eq!(code_metadata.stack_end(), instrumented_code.stack_end);
            assert_eq!(
                code_metadata.instrumentation_status(),
                InstrumentationStatus::NotInstrumented
            );
            assert_eq!(code_metadata.instrumented_code_len(), None);

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
