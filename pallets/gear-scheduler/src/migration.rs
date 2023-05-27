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

//! Database migration module.

use crate::{pallet, Config, Pallet, Weight};
use frame_support::traits::{Get, GetStorageVersion, OnRuntimeUpgrade};
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod v1 {
    use crate::{Config, Pallet};
    use common::storage::ValueStorage;
    use frame_support::{pallet_prelude::StorageValue, traits::PalletInfo};
    use frame_system::pallet_prelude::BlockNumberFor;
    use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

    // BTreeSet used to exclude duplicates and always keep collection sorted.
    /// Missed blocks collection type.
    ///
    /// Defines block number, which should already contain no tasks,
    /// because they were processed before.
    /// Missed blocks processing prioritized.
    pub type MissedBlocksCollection<T> = BTreeSet<BlockNumberFor<T>>;

    pub struct MissedBlocksPrefix<T>(PhantomData<(T,)>);

    impl<T: Config> frame_support::traits::StorageInstance for MissedBlocksPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "MissedBlocks";
    }

    // Private storage for missed blocks collection.
    pub type MissedBlocks<T> = StorageValue<MissedBlocksPrefix<T>, MissedBlocksCollection<T>>;

    // Public wrap of the missed blocks collection.
    common::wrap_storage_value!(
        storage: MissedBlocks,
        name: MissedBlocksWrap,
        value: MissedBlocksCollection<T>
    );
}

pub struct MigrateToV2<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        let mut weight = T::DbWeight::get().reads(1); // 1 read for on chain storage version.

        if current == 2 && onchain == 1 {
            let set = v1::MissedBlocks::<T>::take();
            let bn = set.and_then(|set| set.first().copied());
            pallet::FirstIncompleteTasksBlock::<T>::set(bn);

            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage from v1 to v2");

            // 1 read for `MissedBlocks`
            // 1 write for `FirstIncompleteTasksBlock`
            weight += T::DbWeight::get().reads_writes(1, 1)
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        use parity_scale_codec::Encode;

        let set = v1::MissedBlocks::<T>::get();
        assert!(!pallet::FirstIncompleteTasksBlock::<T>::exists());
        Ok(set.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        use parity_scale_codec::Decode;

        assert!(!v1::MissedBlocks::<T>::exists());
        let first_incomplete_tasks_block = pallet::FirstIncompleteTasksBlock::<T>::get();
        let set: Option<v1::MissedBlocksCollection<T>> = Decode::decode(&mut &state[..]).unwrap();
        assert_eq!(
            first_incomplete_tasks_block,
            set.and_then(|set| set.first().copied())
        );
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use common::storage::ValueStorage;
    use frame_support::traits::StorageVersion;

    #[test]
    fn migrate_to_v2() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<Pallet<Test>>();

            v1::MissedBlocksWrap::<Test>::put([1_u32, 2, 3, 6, 7, 8].map(Into::into).into());

            let state = MigrateToV2::<Test>::pre_upgrade().unwrap();
            let weight = MigrateToV2::<Test>::on_runtime_upgrade();
            assert_ne!(weight.ref_time(), 0);
            MigrateToV2::<Test>::post_upgrade(state).unwrap();

            assert_eq!(
                pallet::FirstIncompleteTasksBlock::<Test>::get(),
                Some(1_u32.into())
            );
        })
    }
}
