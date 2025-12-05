// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::Runtime;

/// All migrations that will run on the next runtime upgrade for dev chain.
#[cfg(feature = "dev")]
pub type Migrations = (
    pallet_gear_eth_bridge::migrations::set_hash::Migration<Runtime>,
    // migrate to v3 of the Gear Scheduler with removal of program pause tasks
    pallet_gear_scheduler::migrations::v3_remove_program_pause_tasks::MigrateRemoveProgramPauseTasks<Runtime>,
);

/// All migrations that will run on the next runtime upgrade for prod chain.
#[cfg(not(feature = "dev"))]
pub type Migrations = (
    pallet_gear_eth_bridge::migrations::set_hash::Migration<Runtime>,
	LockEdForBuiltin<crate::GearEthBridgeBuiltinAddress>,
	// migrate to v3 of the Gear Scheduler with removal of program pause tasks
    pallet_gear_scheduler::migrations::v3_remove_program_pause_tasks::MigrateRemoveProgramPauseTasks<Runtime>,
);

/// This migration is used to top up the ED for the builtin actor from the treasury,
/// applying a lock under EXISTENTIAL_DEPOSIT_LOCK_ID, similar to programs.
#[allow(unused)]
pub struct LockEdForBuiltin<A: sp_core::Get<crate::AccountId>>(core::marker::PhantomData<A>);

impl<A: sp_core::Get<crate::AccountId>> frame_support::traits::OnRuntimeUpgrade
    for LockEdForBuiltin<A>
{
    fn on_runtime_upgrade() -> frame_support::weights::Weight {
        use frame_support::{
            traits::{
                Currency, ExistenceRequirement, InspectLockableCurrency, LockableCurrency,
                WithdrawReasons,
            },
            weights::Weight,
        };
        use pallet_gear::EXISTENTIAL_DEPOSIT_LOCK_ID;
        use runtime_primitives::AccountId;
        use sp_runtime::traits::Zero;

        let mut weight = Weight::zero();

        let weights = <Runtime as frame_system::Config>::DbWeight::get();

        let bia = A::get();

        let bia_lock = <pallet_balances::Pallet<Runtime> as InspectLockableCurrency<AccountId>>::balance_locked(EXISTENTIAL_DEPOSIT_LOCK_ID, &bia);

        if !bia_lock.is_zero() {
            log::info!("Builtin actor {bia:?} already has ed lock. Skipping locking");
            return weights.reads(1);
        }

        let treasury = <pallet_treasury::Pallet<Runtime>>::account_id();
        let ed = <pallet_balances::Pallet<Runtime> as Currency<AccountId>>::minimum_balance();

        if let Err(e) = <pallet_balances::Pallet<Runtime> as Currency<AccountId>>::transfer(
            &treasury,
            &bia,
            ed,
            ExistenceRequirement::KeepAlive,
        ) {
            log::error!("failed to transfer ed from {treasury:?} to builtin actor {bia:?}: {e:?}");
        } else {
            weight = weight.saturating_add(<<Runtime as pallet_balances::Config>::WeightInfo as pallet_balances::WeightInfo>::transfer_keep_alive());

            // Set lock to avoid accidental account removal by the runtime.
            <pallet_balances::Pallet<Runtime> as LockableCurrency<AccountId>>::set_lock(
                EXISTENTIAL_DEPOSIT_LOCK_ID,
                &bia,
                ed,
                WithdrawReasons::all(),
            );

            log::info!("Successfully locked ed for builtin actor {bia:?}");

            weight = weight.saturating_add(weights.reads_writes(1, 1));
        }

        weight
    }
}
