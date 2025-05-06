// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{Config, Pallet};
use common::Origin;
use core::marker::PhantomData;
use frame_support::{
    pallet_prelude::Weight,
    traits::{
        Currency, ExistenceRequirement, Get, GetStorageVersion, LockableCurrency, OnRuntimeUpgrade,
        WithdrawReasons,
    },
};
use pallet_gear::EXISTENTIAL_DEPOSIT_LOCK_ID;

/// TODO: Remove pallet-balances dependency on removal.
pub struct MigrateToV1<T, F>(PhantomData<(T, F)>);

impl<T, F> OnRuntimeUpgrade for MigrateToV1<T, F>
where
    T: Config + pallet_balances::Config,
    T::AccountId: Origin,
    F: Get<T::AccountId>,
{
    fn on_runtime_upgrade() -> Weight {
        let mut weight = Weight::zero();

        let current = Pallet::<T>::in_code_storage_version();
        let on_chain = Pallet::<T>::on_chain_storage_version();

        weight = weight.saturating_add(T::DbWeight::get().reads(1));

        if current == 1 && on_chain == 0 {
            current.put::<Pallet<T>>();
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            let from = F::get();
            let ed = <pallet_balances::Pallet<T> as Currency<T::AccountId>>::minimum_balance();

            for bia in Pallet::<T>::list_builtins() {
                if let Err(e) = <pallet_balances::Pallet<T> as Currency<T::AccountId>>::transfer(
                    &from,
                    &bia,
                    ed,
                    ExistenceRequirement::AllowDeath,
                ) {
                    log::error!(
                        "failed to transfer ed from {from:?} to builtin actor {bia:?}: {e:?}"
                    );
                } else {
                    weight = weight.saturating_add(<<T as pallet_balances::Config>::WeightInfo as pallet_balances::WeightInfo>::transfer_keep_alive());

                    // Set lock to avoid accidental account removal by the runtime.
                    <pallet_balances::Pallet<T> as LockableCurrency<T::AccountId>>::set_lock(
                        EXISTENTIAL_DEPOSIT_LOCK_ID,
                        &bia,
                        ed,
                        WithdrawReasons::all(),
                    );

                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                }
            }

            log::info!("v1 migration applied");
        } else {
            log::warn!("v1 migration is not applicable and should be removed");
        }

        weight
    }
}
