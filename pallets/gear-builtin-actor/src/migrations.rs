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

use crate::*;
use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
};

#[cfg(feature = "try-runtime")]
use {
    frame_support::codec::{Decode, Encode},
    sp_runtime::TryRuntimeError,
    sp_std::vec::Vec,
};

pub mod v1 {
    use super::*;

    pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
    impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            let version = <Pallet<T>>::on_chain_storage_version();

            assert!(Actors::<T>::iter().next().is_none());

            Ok(version.encode())
        }

        fn on_runtime_upgrade() -> Weight {
            let current = Pallet::<T>::current_storage_version();
            let onchain = Pallet::<T>::on_chain_storage_version();

            log::info!(
                "🚚 Running migration with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            // 1 read for onchain storage version
            #[allow(unused_mut)]
            let mut weight = T::DbWeight::get().reads(1);

            if current == 1 && onchain == 0 {
                // TODO: Register builtin actor implementations one by one.
                // match Pallet::<T>::register_actor::<MyBuiltinActor, _, _>() {
                //     Some(_) => {
                //         log::info!("✅ Builtin actor with ID {:?} registered successfully.",
                //         <MyBuiltinActor as RegisteredBuiltinActor<_, _>::ID);
                //         weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                //     }
                //     Err(e) => {
                //         log::info!("❌ Failure to register a builtin actor: {:?}.", e);
                //         weight = weight.saturating_add(T::DbWeight::get().reads(1));
                //     }
                // };

                log::info!("✅ GearBuiltinActor pallet upgraded");

                current.put::<Pallet<T>>();
            } else {
                log::info!("❌ Migration did not execute. This probably should be removed");
            }

            weight
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
            let old_version: StorageVersion =
                Decode::decode(&mut state.as_ref()).expect("Valid state from pre_upgrade; qed");
            let onchain_version = Pallet::<T>::on_chain_storage_version();
            assert_ne!(
                onchain_version, old_version,
                "must have upgraded from version 0 to 1."
            );

            log::info!("Storage successfully migrated.");
            Ok(())
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::mock::{Test as T, *};
    use frame_support::traits::OnRuntimeUpgrade;

    #[test]
    fn migration_to_v1_works() {
        new_test_ext().execute_with(|| {
            // run migration from v0 to v1.
            v1::MigrateToV1::<T>::on_runtime_upgrade();

            // Run necessary checks.
        });
    }
}
