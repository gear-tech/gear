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
use frame_support::traits::{Get, StorageVersion};

mod v1 {
    use crate::{Config, Pallet};
    use common::storage::ValueStorage;
    use frame_support::{pallet_prelude::StorageValue, traits::PalletInfo};
    use frame_system::pallet_prelude::BlockNumberFor;
    use std::{collections::BTreeSet, marker::PhantomData};

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

fn migrate_to_v2<T: Config>() -> Weight {
    pallet::FirstIncompleteTasksBlock::<T>::translate(
        |set: Option<v1::MissedBlocksCollection<T>>| {
            let set = set?;
            let bn = set.first().copied()?;
            Some(bn)
        },
    )
    .unwrap_or_else(|()| {
        unreachable!("Failed to decode old value as `v1::MissedBlocksCollection<T>`")
    });

    StorageVersion::new(2).put::<Pallet<T>>();

    T::DbWeight::get().reads_writes(1, 1)
}

/// Wrapper for all migrations of this pallet, based on `StorageVersion`.
pub fn migrate<T: Config>() -> Weight {
    let version = StorageVersion::get::<Pallet<T>>();
    if version == StorageVersion::new(1) {
        migrate_to_v2::<T>()
    } else {
        Weight::zero()
    }
}
