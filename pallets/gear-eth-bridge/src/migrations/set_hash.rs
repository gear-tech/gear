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

//! This migration puts hash of the current authority set into the storage.

use crate::{AuthoritySetHash, Config};
use frame_support::{
    Blake2_256, StorageHasher,
    pallet_prelude::Weight,
    traits::{Get, OnRuntimeUpgrade},
};
use gprimitives::H256;
use sp_runtime::RuntimeAppPublic;
#[cfg(feature = "try-runtime")]
use {sp_runtime::TryRuntimeError, sp_std::vec::Vec};

pub struct Migration<T>(sp_std::marker::PhantomData<T>);

impl<T: Config + pallet_grandpa::Config> OnRuntimeUpgrade for Migration<T> {
    fn on_runtime_upgrade() -> Weight {
        let mut weight = Weight::zero();
        let db_weight = T::DbWeight::get();

        let authority_set = pallet_grandpa::Pallet::<T>::grandpa_authorities();
        let keys_bytes = authority_set
            .into_iter()
            .flat_map(|(key, _weight)| key.to_raw_vec())
            .collect::<Vec<_>>();

        let grandpa_set_hash = H256::from(Blake2_256::hash(&keys_bytes));

        AuthoritySetHash::<T>::put(grandpa_set_hash);
        weight = weight.saturating_add(db_weight.writes(1));

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok(Default::default())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
        Ok(())
    }
}
