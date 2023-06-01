// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

//! Common runtime migrations.

use frame_support::{
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::marker::PhantomData;

pub struct SessionValidatorSetMigration<T>(PhantomData<T>);

impl<T> OnRuntimeUpgrade for SessionValidatorSetMigration<T>
where
    T: pallet_session::Config
        + validator_set::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
{
    fn on_runtime_upgrade() -> Weight {
        log::info!("🚚 Running migration");

        let mut weight = T::DbWeight::get().reads(
            1 // read pallet session validators
                + 1 // read validator set
                + 1, // read approved validator set
        );

        let session_validators = pallet_session::Pallet::<T>::validators();
        let validator_set = validator_set::Validators::<T>::get();
        let approved_validator_set = validator_set::ApprovedValidators::<T>::get();

        if session_validators == validator_set && session_validators == approved_validator_set {
            log::info!("❌ Migration did not execute. This probably should be removed");
        } else {
            log::info!("Set {} validators", session_validators.len());

            validator_set::Validators::<T>::put(session_validators.clone());
            validator_set::ApprovedValidators::<T>::put(session_validators);

            weight += T::DbWeight::get().writes(
                1 // write validator set
                    + 1, // write approved validator set
            );
        }

        weight
    }
}
