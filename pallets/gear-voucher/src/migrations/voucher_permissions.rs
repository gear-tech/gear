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

use crate::{AccountIdOf, Config, Pallet, VoucherInfo, VoucherPermissions, Vouchers};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::marker::PhantomData;
use std::collections::BTreeSet;
#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 0;
const MIGRATE_TO_VERSION: u16 = 1;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 1;

pub struct VoucherPermissionsMigration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for VoucherPermissionsMigration<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();

        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);
        let mut counter = 0;

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            Vouchers::<T>::translate(
                |_, _, voucher_v0: v0::VoucherInfo<AccountIdOf<T>, BlockNumberFor<T>>| {
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                    counter += 1;

                    let voucher = VoucherInfo {
                        owner: voucher_v0.owner,
                        expiry: voucher_v0.expiry,
                        permissions: VoucherPermissions {
                            programs: voucher_v0.programs,
                            code_uploading: voucher_v0.code_uploading,
                            // By default - deny create porgram from code
                            code_ids: Some(BTreeSet::new()),
                        },
                    };
                    Some(voucher)
                },
            );

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

            Some(v0::Vouchers::<T>::iter_keys().count() as u64)
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
            let count = Vouchers::<T>::iter_keys().count() as u64;
            ensure!(old_count == count, "incorrect count of elements");
        }

        Ok(())
    }
}

mod v0 {
    use crate::AccountIdOf;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use gear_core::ids::ProgramId;
    use sp_runtime::{
        codec::{Decode, Encode},
        scale_info::TypeInfo,
    };
    use sp_std::collections::btree_set::BTreeSet;

    #[cfg(feature = "try-runtime")]
    use {
        crate::{Config, Pallet, VoucherId},
        frame_support::traits::{PalletInfo, StorageInstance},
        sp_std::marker::PhantomData,
    };

    /// Type containing all data about voucher.
    #[derive(Debug, Encode, Decode, TypeInfo)]
    pub struct VoucherInfo<AccountId, BlockNumber> {
        /// Owner of the voucher.
        /// May be different to original issuer.
        /// Owner manages and claims back remaining balance of the voucher.
        pub owner: AccountId,
        /// Set of programs this voucher could be used to interact with.
        /// In case of [`None`] means any gear program.
        pub programs: Option<BTreeSet<ProgramId>>,
        /// Flag if this voucher's covers uploading codes as prepaid call.
        pub code_uploading: bool,
        /// The block number at and after which voucher couldn't be used and
        /// can be revoked by owner.
        pub expiry: BlockNumber,
    }

    #[cfg(feature = "try-runtime")]
    pub struct VouchersPrefix<T>(PhantomData<T>);

    #[cfg(feature = "try-runtime")]
    impl<T: Config> StorageInstance for VouchersPrefix<T> {
        const STORAGE_PREFIX: &'static str = "Vouchers";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    #[cfg(feature = "try-runtime")]
    pub type Vouchers<T> = StorageDoubleMap<
        VouchersPrefix<T>,
        Identity,
        AccountIdOf<T>,
        Identity,
        VoucherId,
        VoucherInfo<AccountIdOf<T>, BlockNumberFor<T>>,
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::{mock::*, VoucherId};
    use frame_support::traits::StorageVersion;
    use sp_runtime::traits::Zero;

    #[test]
    fn voucher_migratge_permissions_works() {
        let _ = env_logger::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<Voucher>();

            let owner = AccountIdOf::<Test>::from(0u64);
            let spender = AccountIdOf::<Test>::from(1u64);
            let expiry = <frame_system::Pallet<Test>>::block_number().saturating_add(100);
            let voucher_id = VoucherId::generate::<Test>();

            let voucher_v0 = v0::VoucherInfo {
                owner,
                programs: None,
                code_uploading: true,
                expiry,
            };

            v0::Vouchers::<Test>::insert(spender, voucher_id, voucher_v0);

            // act
            let state = VoucherPermissionsMigration::<Test>::pre_upgrade().unwrap();
            let w = VoucherPermissionsMigration::<Test>::on_runtime_upgrade();
            VoucherPermissionsMigration::<Test>::post_upgrade(state).unwrap();

            // assert
            assert!(!w.is_zero());

            let voucher_v1 = Vouchers::<Test>::get(spender, voucher_id).unwrap();
            assert_eq!(owner, voucher_v1.owner);
            assert_eq!(expiry, voucher_v1.expiry);
            assert_eq!(None, voucher_v1.permissions.programs);
            assert_eq!(true, voucher_v1.permissions.code_uploading);
            assert_eq!(Some(BTreeSet::new()), voucher_v1.permissions.code_ids);
        });
    }
}
