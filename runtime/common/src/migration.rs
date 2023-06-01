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
    codec::FullCodec,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::marker::PhantomData;

pub struct SessionValidatorSetMigration<Config, Id>(PhantomData<(Config, Id)>);

impl<Config, Id> OnRuntimeUpgrade for SessionValidatorSetMigration<Config, Id>
where
    Config: pallet_session::Config<AccountId = Id> + validator_set::Config<ValidatorId = Id>,
    Id: FullCodec + Clone + 'static,
{
    fn on_runtime_upgrade() -> Weight {
        // TODO: add check migration must be removed

        let current_validators = pallet_session::Pallet::<Config>::validators();
        validator_set::Validators::<Config>::put(current_validators.clone());
        validator_set::ApprovedValidators::<Config>::put(current_validators);

        Config::DbWeight::get().reads_writes(
            1, // read pallet session validators
            1 // write validator set 
            + 1, // write approved validator set
        )
    }
}
