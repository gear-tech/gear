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

    use binary_merkle_tree as merkle_tree;
    use frame_support::{pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;
    use primitive_types::H256;
    use sp_runtime::traits::Zero;

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
        type QueueLimit: Get<u32>;
    }

    // Gear Bridge Pallet event type.
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T> {
        RootUpdated(H256),
        MessageQueued(H256),
    }

    // Gear Bridge Pallet error type.
    #[pallet::error]
    pub enum Error<T> {
        QueueLimitExceeded,
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

    #[pallet::storage]
    pub(crate) type QueueMerkleRoot<T> = StorageValue<_, H256>;

    #[pallet::storage]
    pub(crate) type Queue<T> = StorageValue<_, BoundedVec<H256, <T as Config>::QueueLimit>>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Queues new hash into hash queue.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::zero())]
        pub fn send(origin: OriginFor<T>, hash: H256) -> DispatchResultWithPostInfo {
            let _who = ensure_signed(origin)?;

            Queue::<T>::mutate(|opt| {
                let v = opt.get_or_insert_with(BoundedVec::new);
                v.try_push(hash).map_err(|_| Error::<T>::QueueLimitExceeded)
            })?;

            Self::deposit_event(Event::<T>::MessageQueued(hash));

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// End of the block.
        fn on_finalize(_bn: BlockNumberFor<T>) {
            // Querying non-empty queue.
            let Some(queue) = Queue::<T>::get() else {
                return;
            };

            // Temporary debug assertion.
            debug_assert!(!queue.len().is_zero());

            // Merkle root calculation.
            let root = merkle_tree::merkle_root::<Hasher, _>(queue);

            // Storing new root.
            QueueMerkleRoot::<T>::put(root);

            // Depositing event.
            Self::deposit_event(Event::<T>::RootUpdated(root));
        }
    }
}
