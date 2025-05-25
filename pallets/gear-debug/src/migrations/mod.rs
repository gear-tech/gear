// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::Config;
use frame_support::{
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::marker::PhantomData;

pub struct MigrateRemoveAllStorages<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateRemoveAllStorages<T> {
    fn on_runtime_upgrade() -> Weight {
        // 3 writes for removing 3 storages
        let weight = T::DbWeight::get().writes(3);

        v0::DebugMode::<T>::kill();
        v0::RemapId::<T>::kill();
        v0::ProgramsMap::<T>::kill();

        weight
    }
}

mod v0 {
    use primitive_types::H256;

    use crate::{Config, Pallet};
    use frame_support::{
        pallet_prelude::{StorageValue, ValueQuery},
        traits::{PalletInfo, StorageInstance},
    };
    use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

    // Debug mode storage.
    pub struct DebugModePrefix<T>(PhantomData<T>);
    impl<T: Config> StorageInstance for DebugModePrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }

        const STORAGE_PREFIX: &'static str = "DebugMode";
    }

    pub type DebugMode<T> = StorageValue<DebugModePrefix<T>, bool, ValueQuery>;

    // Remap ID storage.
    pub struct RemapIdPrefix<T>(PhantomData<T>);
    impl<T: Config> StorageInstance for RemapIdPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }

        const STORAGE_PREFIX: &'static str = "RemapId";
    }

    pub type RemapId<T> = StorageValue<RemapIdPrefix<T>, bool, ValueQuery>;

    // Programs map storage.
    pub struct ProgramsMapPrefix<T>(PhantomData<T>);
    impl<T: Config> StorageInstance for ProgramsMapPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }

        const STORAGE_PREFIX: &'static str = "ProgramsMap";
    }
    pub type ProgramsMap<T> = StorageValue<ProgramsMapPrefix<T>, BTreeMap<H256, H256>, ValueQuery>;
}
