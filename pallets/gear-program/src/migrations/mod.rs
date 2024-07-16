// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

pub mod allocations;
pub mod paused_storage;
pub mod v8;

pub mod v8_fix {
    use crate::{migrations::v8::AccountIdOf, Config, Pallet, ProgramStorage};
    use common::{Origin, Program};
    use core::marker::PhantomData;
    use frame_support::{
        pallet_prelude::Weight,
        traits::{
            tokens::Pay, Currency, GetStorageVersion, LockableCurrency, OnRuntimeUpgrade,
            StorageVersion, WithdrawReasons,
        },
    };

    const EXISTENTIAL_DEPOSIT_LOCK_ID: [u8; 8] = *b"glock/ed";

    type CurrencyOf<T> = <T as pallet_gear_bank::Config>::Currency;
    type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;

    pub struct AsapFix<T: Config>(PhantomData<T>);

    impl<T: Config> OnRuntimeUpgrade for AsapFix<T>
    where
        T: pallet_treasury::Config + pallet_gear_bank::Config,
        T::AccountId: Origin,
        T::Paymaster: Pay<Beneficiary = T::AccountId, AssetKind = (), Balance = BalanceOf<T>>,
    {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = Weight::from_parts(0, 0);

            if Pallet::<T>::on_chain_storage_version() == StorageVersion::new(8) {
                log::info!("Running asap fix migrations");

                let ed = CurrencyOf::<T>::minimum_balance();

                ProgramStorage::<T>::iter().for_each(|(program_id, program)| {
                    let program_id: AccountIdOf<T> = program_id.cast();

                    if let Program::Active(_) = program {
                        if CurrencyOf::<T>::total_balance(&program_id) - CurrencyOf::<T>::free_balance(&program_id) != ed
                        {
                            let mut to_set_lock = true;

                            if CurrencyOf::<T>::free_balance(&program_id) < ed {
                                match <T as pallet_treasury::Config>::Paymaster::pay(
                                    &program_id,
                                    (),
                                    ed,
                                ) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        to_set_lock = false;

                                        log::error!(
                                            "❌ Failed to transfer ED and set lock to {program_id:?}: {e:?}"
                                        );
                                    }
                                };
                            }

                            if to_set_lock {
                                CurrencyOf::<T>::set_lock(
                                    EXISTENTIAL_DEPOSIT_LOCK_ID,
                                    &program_id,
                                    ed,
                                    WithdrawReasons::all(),
                                )
                            }
                        }
                    };
                });

                weight = Weight::from_parts(200_000_000, 0);

                log::info!("✅ Successfully fixed migrated storage");
            } else {
                log::info!("🟠 Migration requires onchain version 8, so was skipped");
            }

            weight
        }
    }
}
