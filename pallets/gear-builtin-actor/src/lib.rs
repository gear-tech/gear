// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! # Gear Builtin Actors Pallet
//!
//! The Builtn Actors pallet provides a registry of the builtin actors available in the Runtime.
//!
//! - [`Config`]
//!
//! ## Overview
//!
//! The pallet implements the `pallet_gear::BuiltinLookup` allowing to restore builtin actors
//! claimed `BuiltinId`'s based on their corresponding `ProgramId` address.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]

extern crate alloc;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use common::Origin;
use gear_core::ids::{BuiltinId, ProgramId};
use pallet_gear::{BuiltinLookup, RegisteredBuiltinActor};
use parity_scale_codec::{Decode, Encode};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::TrailingZeroInput;

pub use pallet::*;

#[allow(dead_code)]
const LOG_TARGET: &str = "gear::builtin_actor";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::Get, PalletId};
    use frame_system::pallet_prelude::*;

    /// The current storage version.
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The built-in actor pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;
    }

    /// Cached built-in actor program ids to spare redundant computation.
    #[pallet::storage]
    #[pallet::getter(fn actors)]
    pub type Actors<T> = StorageMap<_, Identity, ProgramId, BuiltinId>;

    /// Error for the gear-builtin-actor pallet.
    #[pallet::error]
    pub enum Error<T> {
        /// `BuiltinId` already existd.
        BuiltinIdAlreadyExists,
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub builtin_ids: Vec<BuiltinId>,
        pub _phantom: sp_std::marker::PhantomData<T>,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                builtin_ids: Default::default(),
                _phantom: Default::default(),
            }
        }
    }

    // TODO: deprecated; remove in Substrate v1.0.0
    #[cfg(feature = "std")]
    impl<T: Config> GenesisConfig<T> {
        /// Direct implementation of `GenesisBuild::assimilate_storage`.
        pub fn assimilate_storage(&self, storage: &mut sp_runtime::Storage) -> Result<(), String> {
            <Self as GenesisBuild<T>>::assimilate_storage(self, storage)
        }
    }

    // TODO: replace with `BuildGenesisConfig` trait in Substrate v1.0.0
    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            self.builtin_ids.iter().cloned().for_each(|id| {
                let actor_id = Pallet::<T>::generate_actor_id(id);
                Actors::<T>::insert(actor_id, id);
            });
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> Pallet<T> {
        /// Generate an `actor_id` given a builtin ID.
        ///
        ///
        /// This does computations, therefore we should seek to cache the value at the time of
        /// a builtin actor registration.
        pub fn generate_actor_id(builtin_id: BuiltinId) -> ProgramId {
            let entropy = (T::PalletId::get(), builtin_id).using_encoded(blake2_256);
            let actor_id = Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed");
            actor_id
        }

        /// Register a builtin actor.
        ///
        /// This function is supposed to be called during the Runtime upgrade to update the
        /// builtin actors cache (if new actors are being added).
        #[allow(unused)]
        pub(crate) fn register_actor<B, D, O>() -> DispatchResult
        where
            B: RegisteredBuiltinActor<D, O>,
        {
            let builtin_id = <B as RegisteredBuiltinActor<D, O>>::ID;
            let actor_id = Self::generate_actor_id(builtin_id);
            ensure!(
                !Actors::<T>::contains_key(actor_id),
                Error::<T>::BuiltinIdAlreadyExists
            );
            Actors::<T>::insert(actor_id, builtin_id);
            Ok(())
        }
    }
}

impl<T: Config> BuiltinLookup<ProgramId> for Pallet<T>
where
    T::AccountId: Origin,
{
    fn lookup(id: &ProgramId) -> Option<BuiltinId> {
        Self::actors(id)
    }
}
