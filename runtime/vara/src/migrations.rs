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
    // substrate v1.4.0
    staking::MigrateToV14<Runtime>,
    pallet_grandpa::migrations::MigrateV4ToV5<Runtime>,
    // move metadata into attribution
    pallet_gear_program::migrations::v11_metadata_into_attribution::MigrateMetadataIntoAttribution<Runtime>,
    // migrate program code hash to code id and remove code_exports and static_pages
    pallet_gear_program::migrations::v12_program_code_id_migration::MigrateProgramCodeHashToCodeId<Runtime>,
    // split instrumented code into separate storage items
    pallet_gear_program::migrations::v13_split_instrumented_code_migration::MigrateSplitInstrumentedCode<Runtime>,
);

mod staking {
    use frame_support::{
        pallet_prelude::Weight,
        traits::{GetStorageVersion, OnRuntimeUpgrade},
    };
    use pallet_staking::*;
    use sp_core::Get;

    #[cfg(feature = "try-runtime")]
    use sp_std::vec::Vec;

    #[cfg(feature = "try-runtime")]
    use sp_runtime::TryRuntimeError;

    pub struct MigrateToV14<T>(sp_std::marker::PhantomData<T>);
    impl<T: Config> OnRuntimeUpgrade for MigrateToV14<T> {
        fn on_runtime_upgrade() -> Weight {
            let current = Pallet::<T>::current_storage_version();
            let on_chain = Pallet::<T>::on_chain_storage_version();

            if current == 14 && on_chain == 13 {
                current.put::<Pallet<T>>();

                log::info!("v14 applied successfully.");
                T::DbWeight::get().reads_writes(1, 1)
            } else {
                log::warn!("v14 not applied.");
                T::DbWeight::get().reads(1)
            }
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            Ok(Default::default())
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
            frame_support::ensure!(
                Pallet::<T>::on_chain_storage_version() == 14,
                "v14 not applied"
            );
            Ok(())
        }
    }
}
