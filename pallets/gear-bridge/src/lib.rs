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

//! # Gear Bridge Pallet

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

// Runtime mock for running tests.
#[cfg(test)]
mod mock;

// Unit tests module.
#[cfg(test)]
mod tests;

// Public exports from pallet.
pub use pallet::*;

// Gear Bridge Pallet module.
#[frame_support::pallet]
pub mod pallet {
    pub use frame_support::weights::Weight;

    pub type Hasher = sp_runtime::traits::Keccak256;

    #[allow(unused)]
    use binary_merkle_tree as merkle_tree;
    use frame_support::{pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;

    /// The current storage version.
    pub const BRIDGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    /// Gear Bridge Pallet's `Config`.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>>
            + TryInto<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Limit of messages to be bridged withing the era.
        #[pallet::constant]
        type Limit: Get<u32>;
    }

    // Gear Bridge Pallet event type.
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T> {
        ImAlive,
    }

    // Gear Bridge Pallet error type.
    #[pallet::error]
    pub enum Error<T> {
        ImDead,
    }

    // Gear Bridge Pallet itself.
    //
    // Uses without storage info to avoid direct access to pallet's
    // storage from outside.
    //
    // Uses `BRIDGE_STORAGE_VERSION` as current storage version.
    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(BRIDGE_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Start of the block.
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            Weight::zero()
        }

        /// End of the block.
        fn on_finalize(_bn: BlockNumberFor<T>) {
            Self::deposit_event(Event::<T>::ImAlive);
        }
    }
}
