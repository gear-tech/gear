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

use crate::{Config, Pallet, ProgramStorage};
use common::Origin;
use frame_support::{
    traits::{tokens::Pay, Currency, Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use gear_core::program::Program;
#[cfg(feature = "try-runtime")]
use pallet_balances::Pallet as Balances;
use pallet_balances::WeightInfo;
#[cfg(feature = "try-runtime")]
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 7;
const MIGRATE_TO_VERSION: u16 = 8;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 8;

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type CurrencyOf<T> = <T as pallet_treasury::Config>::Currency;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;

pub struct MigrateToV8<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV8<T>
where
    T: pallet_treasury::Config + pallet_balances::Config,
    T::AccountId: Origin,
    T::Paymaster: Pay<Beneficiary = T::AccountId, AssetKind = (), Balance = BalanceOf<T>>,
{
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();
        let existential_deposit = CurrencyOf::<T>::minimum_balance();
        let transfer_weight = <T as pallet_balances::Config>::WeightInfo::transfer_allow_death();

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();

            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            let mut transferred = 0_u32;
            let mut active_count = 0_u32;
            ProgramStorage::<T>::iter().for_each(|(program_id, program)| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                if let Program::Active(_p) = program {
                    active_count += 1;
                    // Transfer ED from treasury account to program's account
                    // regardless whether the latter is extant or not.
                    match <T as pallet_treasury::Config>::Paymaster::pay(
                        &program_id.cast(),
                        (),
                        existential_deposit,
                    ) {
                        Ok(_res) => {
                            transferred += 1;
                        }
                        Err(e) => {
                            log::error!(
                                "❌ Failed to transfer ED to program {program_id:?}: {e:?}"
                            );
                        }
                    };
                    weight = weight.saturating_add(transfer_weight);
                };
            });

            log::debug!(
                "🚚 Updated balances for {transferred:?} out of {active_count:?} active programs."
            );

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage");
        } else {
            log::info!("🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::debug!("[MigrateToV8::pre_upgrade] current: {current:?}, onchain: {onchain:?}");
        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(
                ProgramStorage::<T>::iter()
                    .filter_map(|(program_id, program)| match program {
                        Program::Active(_p) => {
                            Some(Balances::<T>::free_balance(&program_id.cast()))
                        }
                        _ => None,
                    })
                    .fold((0_u128, 0_u64), |(sum, count), balance| {
                        (
                            sum.saturating_add(balance.unique_saturated_into()),
                            count + 1,
                        )
                    }),
            )
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let ed: u128 = CurrencyOf::<T>::minimum_balance().unique_saturated_into();

        if let Some((old_sum, old_count)) = Option::<(u128, u64)>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            log::debug!(
                "[MigrateToV8::post_upgrade] old_sum: {old_sum:?}, old_count: {old_count:?}"
            );
            let (new_sum, new_count) = ProgramStorage::<T>::iter()
                .filter_map(|(program_id, program)| match program {
                    Program::Active(_p) => Some(Balances::<T>::free_balance(&program_id.cast())),
                    _ => None,
                })
                .fold((0_u128, 0_u64), |(sum, count), balance| {
                    (
                        sum.saturating_add(balance.unique_saturated_into()),
                        count + 1,
                    )
                });
            log::debug!(
                "[MigrateToV8::post_upgrade] new_sum: {new_sum:?}, new_count: {new_count:?}"
            );
            ensure!(
                new_count == old_count,
                "Active programs count does not match after upgrade: {} != {}",
            );
            let expected_sum = old_sum + old_count as u128 * ed;
            ensure!(
                new_sum == expected_sum,
                "Active programs total balance is not as expected: {} != {}",
            );
        }

        Ok(())
    }
}

#[cfg(all(feature = "try-runtime", test))]
mod tests {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{
        ids::ProgramId,
        program::{ActiveProgram, ProgramState},
    };
    use pallet_treasury::Pallet as Treasury;
    use sp_runtime::traits::Zero;

    #[test]
    fn migration_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();
            let treasury_account = Treasury::<Test>::account_id();
            // Mint enough funds to the treasury account
            let _ = CurrencyOf::<Test>::deposit_creating(&treasury_account, 1000 * UNITS);

            const NUM_ACTIVE_PROGRAMS: u64 = 690; // Close to actual number of programs in the Runtime
            const NUM_EXITED_PROGRAMS: u64 = 50;
            const NUM_TERMINATED_PROGRAMS: u64 = 30;

            // Populate program storage with active programs
            for i in 0_u64..NUM_ACTIVE_PROGRAMS {
                let program_id = ProgramId::from(1000_u64 + i);
                let program = Program::<BlockNumberFor<Test>>::Active(ActiveProgram {
                    allocations_tree_len: 0,
                    gas_reservation_map: Default::default(),
                    code_hash: Default::default(),
                    code_exports: Default::default(),
                    static_pages: 1.into(),
                    state: ProgramState::Initialized,
                    expiration_block: 100,
                    memory_infix: Default::default(),
                });
                ProgramStorage::<Test>::insert(program_id, program);
                // For half of the programs, add some balance to the account
                if i % 2 == 0 {
                    let _ = CurrencyOf::<Test>::deposit_creating(&program_id.cast(), 100 * UNITS);
                }
            }

            // Add exited programs
            for i in 0_u64..NUM_EXITED_PROGRAMS {
                let program_id = ProgramId::from(2000_u64 + i);
                let program = Program::<BlockNumberFor<Test>>::Exited(program_id);
                ProgramStorage::<Test>::insert(program_id, program);
            }

            // Add terminated programs
            for i in 0_u64..NUM_TERMINATED_PROGRAMS {
                let program_id = ProgramId::from(3000_u64 + i);
                let program = Program::<BlockNumberFor<Test>>::Terminated(program_id);
                ProgramStorage::<Test>::insert(program_id, program);
            }

            // Take note of the total issuance before the upgrade
            let total_issuance = CurrencyOf::<Test>::total_issuance();
            // Treasury balance before the upgrade
            let treasury_balance = CurrencyOf::<Test>::free_balance(treasury_account);

            // Run the migration
            let state = MigrateToV8::<Test>::pre_upgrade().unwrap();
            let weight = MigrateToV8::<Test>::on_runtime_upgrade();
            println!("Weight: {:?}", weight);
            assert!(!weight.is_zero());
            MigrateToV8::<Test>::post_upgrade(state).unwrap();

            // Check that balances of the active programs add up
            let ed = CurrencyOf::<Test>::minimum_balance();
            for i in 0_u64..NUM_ACTIVE_PROGRAMS {
                let program_id = ProgramId::from(1000_u64 + i);
                let balance =
                    CurrencyOf::<Test>::free_balance(program_id.cast::<AccountIdOf<Test>>());
                if i % 2 == 0 {
                    assert_eq!(balance, ed.saturating_add(100 * UNITS));
                } else {
                    assert_eq!(balance, ed);
                }
            }

            // Total issuance should have remained intact
            assert_eq!(CurrencyOf::<Test>::total_issuance(), total_issuance);

            // Treasury balance should have decreased by the total EDs transferred
            let expected_treasury_balance =
                treasury_balance.saturating_sub(ed.saturating_mul(NUM_ACTIVE_PROGRAMS as u128));
            assert_eq!(
                CurrencyOf::<Test>::free_balance(treasury_account),
                expected_treasury_balance
            );

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
