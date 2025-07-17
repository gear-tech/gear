// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{BankAddress, Config, Pallet};
use common::Origin;
#[cfg(feature = "try-runtime")]
use frame_support::storage::generator::StorageValue;
use frame_support::{
    pallet_prelude::Weight,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, OriginTrait},
};
use frame_system::pallet_prelude::OriginFor;
use pallet_balances::{Pallet as Balances, WeightInfo};
use sp_runtime::traits::StaticLookup;

#[cfg(feature = "try-runtime")]
use {
    frame_support::{ensure, traits::Currency},
    sp_runtime::{
        TryRuntimeError,
        codec::{Decode, Encode},
        traits::Zero,
    },
    sp_std::vec::Vec,
};

const OLD_BANK_ADDRESS: [u8; 32] = *b"gearbankgearbankgearbankgearbank";

#[cfg(feature = "try-runtime")]
pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
#[cfg(feature = "try-runtime")]
pub(crate) type BalanceOf<T> = <<T as Config>::Currency as Currency<AccountIdOf<T>>>::Balance;

pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T>
where
    T: pallet_balances::Config,
    T::AccountId: Origin,
{
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::in_code_storage_version();
        let on_chain = Pallet::<T>::on_chain_storage_version();

        if current == 1 && on_chain == 0 {
            current.put::<Pallet<T>>();

            // Transfer all funds from the old gear bank account to the new one
            if Balances::<T>::transfer_all(
                OriginFor::<T>::signed(T::AccountId::from_origin(OLD_BANK_ADDRESS.into())),
                T::Lookup::unlookup(Pallet::<T>::bank_address()),
                false,
            )
            .is_ok()
            {
                log::info!("Migration to v1 applied successfully.");
            } else {
                log::error!("Migration to v1 failed");
            }

            // Mutate the bank address in storage
            BankAddress::<T>::put(Pallet::<T>::bank_address());

            // Two writes are: the new on-chain storage version and the new bank address
            <T as pallet_balances::Config>::WeightInfo::transfer_all()
                .saturating_add(T::DbWeight::get().reads_writes(2, 2))
        } else {
            log::warn!("v1 migration is not applicable.");
            T::DbWeight::get().reads(2)
        }
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if current == 1 && onchain == 0 {
            Some((
                T::Currency::free_balance(&T::AccountId::from_origin(OLD_BANK_ADDRESS.into())),
                T::Currency::free_balance(&Pallet::<T>::bank_address()),
            ))
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if let Some(old_balances) =
            Option::<(BalanceOf<T>, BalanceOf<T>)>::decode(&mut state.as_ref())
                .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            let new_balances = (
                T::Currency::free_balance(&T::AccountId::from_origin(OLD_BANK_ADDRESS.into())),
                T::Currency::free_balance(&Pallet::<T>::bank_address()),
            );

            ensure!(
                new_balances.0 == BalanceOf::<T>::zero(),
                "Old gear bank address still has funds"
            );
            ensure!(
                new_balances.1 == old_balances.0,
                "Balance at destination is different from the source"
            );
            ensure!(
                Pallet::<T>::on_chain_storage_version() == 1,
                "v1 not applied"
            );

            // Ensure the new bank address has been written to storage
            let prefix = BankAddress::<T>::storage_value_final_key();
            frame_support::storage::unhashed::get::<T::AccountId>(&prefix)
                .ok_or("Bank address not found in storage")?;
        }

        Ok(())
    }
}

#[cfg(all(feature = "try-runtime", test))]
mod tests {
    use super::*;
    use crate as pallet_gear_bank;
    use frame_support::{
        PalletId, assert_ok, construct_runtime, parameter_types,
        traits::{ConstU32, FindAuthor, StorageVersion},
        weights::constants::RocksDbWeight,
    };
    use primitive_types::H256;
    use sp_runtime::{
        AccountId32, BuildStorage,
        traits::{BlakeTwo256, IdentityLookup},
    };

    static BLOCK_AUTHOR: AccountId32 = AccountId32::new(*b"blk/author/blk/author/blk/author");
    const EXISTENTIAL_DEPOSIT: Balance = 100_000;

    type AccountId = AccountId32;
    type Block = frame_system::mocking::MockBlock<Test>;
    type Balance = u128;
    type BlockNumber = u64;

    parameter_types! {
        pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
        pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(1);
        pub const BlockHashCount: BlockNumber = 250;
        pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
        pub const TreasuryAddress: AccountId = AccountId32::new(*b"treasuryaccount/treasuryaccount/");
    }

    construct_runtime!(
        pub enum Test
        {
            System: frame_system,
            Authorship: pallet_authorship,
            Balances: pallet_balances,
            GearBank: pallet_gear_bank,
        }
    );

    common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
    common::impl_pallet_balances!(Test);
    pallet_gear_bank::impl_config!(Test, TreasuryAddress = TreasuryAddress);

    pub struct FixedBlockAuthor;
    impl FindAuthor<AccountId> for FixedBlockAuthor {
        fn find_author<'a, I: 'a>(_: I) -> Option<AccountId> {
            Some(BLOCK_AUTHOR.clone())
        }
    }
    impl pallet_authorship::Config for Test {
        type FindAuthor = FixedBlockAuthor;
        type EventHandler = ();
    }

    pub fn new_test_ext() -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap();

        let balances = vec![(OLD_BANK_ADDRESS.into(), EXISTENTIAL_DEPOSIT)];

        pallet_balances::GenesisConfig::<Test> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();

        // Note: pallet_gear_bank GenesisConfig is deliberately not applied to simulate
        // current on-chain situation where the bank address is not present in storage.

        sp_io::TestExternalities::new(storage)
    }

    #[test]
    fn migration_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(0).put::<GearBank>();
            let new_bank_address = GearBank::bank_address();
            // Pour some more funds into the Gear bank
            let _ = Balances::deposit_creating(&OLD_BANK_ADDRESS.into(), 1_000_000);

            // Take note of the current total issuance before the upgrade
            let total_issuance = Balances::total_issuance();
            // New Gear bank account balance before the upgrade should be 0
            assert_eq!(Balances::free_balance(&new_bank_address), 0);

            // Run the migration
            let state = MigrateToV1::<Test>::pre_upgrade().unwrap();
            let weight = MigrateToV1::<Test>::on_runtime_upgrade();
            assert_ok!(MigrateToV1::<Test>::post_upgrade(state));

            println!("Weight: {weight:?}");
            assert!(!weight.is_zero());

            assert_eq!(StorageVersion::get::<GearBank>(), 1);

            // Total issuance should have remained intact
            assert_eq!(Balances::total_issuance(), total_issuance);

            // Check that balances add up
            assert_eq!(
                Balances::free_balance(AccountId32::from(OLD_BANK_ADDRESS)),
                0,
                "Old bank address should be empty"
            );
            assert_eq!(
                Balances::free_balance(&new_bank_address),
                1_000_000 + EXISTENTIAL_DEPOSIT,
                "New bank address should have the funds"
            );
        })
    }
}
