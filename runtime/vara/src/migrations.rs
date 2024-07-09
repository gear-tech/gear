// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::*;

/// All migrations that will run on the next runtime upgrade.
pub type Migrations = (
    // migrate allocations from BTreeSet to IntervalsTree
    pallet_gear_program::migrations::allocations::MigrateAllocations<Runtime>,
    // migration for removed paused program storage
    pallet_gear_program::migrations::paused_storage::RemovePausedProgramStorageMigration<Runtime>,
    pallet_gear_program::migrations::v8::MigrateToV8<Runtime>,
    // migration for added section sizes
    pallet_gear_program::migrations::add_section_sizes::AddSectionSizesMigration<Runtime>,
    // substrate v1.4.0
    pallet_staking::migrations::v14::MigrateToV14<Runtime>,
    pallet_grandpa::migrations::MigrateV4ToV5<Runtime>,
);

mod staking_v13 {
    use frame_support::{
        pallet_prelude::{ValueQuery, Weight},
        storage_alias,
        traits::{GetStorageVersion, OnRuntimeUpgrade},
    };
    use pallet_staking::*;
    use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
    use scale_info::TypeInfo;
    use sp_core::Get;
    use sp_std::vec::Vec;

    #[cfg(feature = "try-runtime")]
    use sp_runtime::TryRuntimeError;

    /// Alias to the old storage item used for release versioning. Obsolete since v13.
    #[storage_alias]
    type StorageVersion<T: pallet_staking::Config> =
        StorageValue<Pallet<T>, ObsoleteReleases, ValueQuery>;

    /// Used for release versioning upto v12.
    ///
    /// Obsolete from v13. Keeping around to make encoding/decoding of old migration code easier.
    #[derive(Default, Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
    enum ObsoleteReleases {
        V1_0_0Ancient,
        V2_0_0,
        V3_0_0,
        V4_0_0,
        V5_0_0,  // blockable validators.
        V6_0_0,  // removal of all storage associated with offchain phragmen.
        V7_0_0,  // keep track of number of nominators / validators in map
        V8_0_0,  // populate `VoterList`.
        V9_0_0,  // inject validators into `VoterList` as well.
        V10_0_0, // remove `EarliestUnappliedSlash`.
        V11_0_0, // Move pallet storage prefix, e.g. BagsList -> VoterBagsList
        V12_0_0, // remove `HistoryDepth`.
        #[default]
        V13_0_0, // Force migration from `ObsoleteReleases`.
    }

    pub struct MigrateToV13<T>(sp_std::marker::PhantomData<T>);
    impl<T: pallet_staking::Config> OnRuntimeUpgrade for MigrateToV13<T> {
        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            frame_support::ensure!(
                StorageVersion::<T>::get() == ObsoleteReleases::V13_0_0,
                "Required ObsoleteReleases::V13_0_0 before upgrading to v13"
            );

            Ok(Default::default())
        }

        fn on_runtime_upgrade() -> Weight {
            let current = Pallet::<T>::current_storage_version();
            let onchain = StorageVersion::<T>::get();

            if current == 13 && onchain == ObsoleteReleases::V13_0_0 {
                StorageVersion::<T>::kill();
                current.put::<Pallet<T>>();

                log::info!("v13 applied successfully");
                T::DbWeight::get().reads_writes(1, 2)
            } else {
                log::warn!("Skipping v13, should be removed");
                T::DbWeight::get().reads(1)
            }
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
            frame_support::ensure!(
                Pallet::<T>::on_chain_storage_version() >= 13,
                "v13 not applied"
            );

            frame_support::ensure!(
                !StorageVersion::<T>::exists(),
                "Storage version not migrated correctly"
            );

            Ok(())
        }
    }
}
