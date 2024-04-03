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

// Module with internal implementation details.
mod internal;

// Public exports from pallet.
pub use pallet::*;

// Gear Bridge Pallet module.
#[frame_support::pallet]
pub mod pallet {
    use crate::internal::{EthMessage, EthMessageData, FirstNonce};
    use binary_merkle_tree as merkle_tree;
    use common::Origin;
    use frame_support::{pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;
    use gear_core::message::PayloadSizeError;
    use primitive_types::{H160, H256, U256};

    pub type Hasher = sp_runtime::traits::Keccak256;

    pub use frame_support::weights::Weight;

    /// The current storage version.
    pub const BRIDGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    /// Amount of trailing zeroes kept with queue root.
    ///
    /// SAFETY: changing this param will require significant changes around the
    /// bridge and root value migration.
    pub const TRAILING_ZEROES: usize = 1;

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
        RootReset,
        RootUpdated(H256),
        MessageQueued { message: EthMessage, hash: H256 },
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
    pub(crate) type AcceptRequests<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub(crate) type Nonce<T> = StorageValue<_, U256, ValueQuery, FirstNonce>;

    #[pallet::storage]
    pub(crate) type Queue<T> =
        StorageValue<_, BoundedVec<H256, <T as Config>::QueueLimit>, ValueQuery>;

    #[pallet::storage]
    pub(crate) type QueueChanged<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub(crate) type QueueMerkleRoot<T> = StorageValue<_, [u8; 32 + TRAILING_ZEROES]>;

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::zero())]
        pub fn switch(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            AcceptRequests::<T>::mutate(|v| *v = !*v);

            Ok(().into())
        }

        /// Queues new hash into hash queue.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::zero())]
        pub fn send(
            origin: OriginFor<T>,
            destination: H160,
            payload: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let payload = payload
                .try_into()
                .map_err(|e: PayloadSizeError| DispatchError::Other(e.into()))?;

            let data = EthMessageData::new(destination, payload);

            let nonce = Nonce::<T>::mutate(|v| {
                let res = *v;
                *v = v.saturating_add(U256::one());
                res
            });

            let source = who.into_origin();

            let message = EthMessage::from_data(source, data, nonce);

            let hash = Queue::<T>::mutate(|v| {
                (v.len() < T::QueueLimit::get() as usize)
                    .then(|| {
                        let hash = message.hash();

                        // Always `Ok`: check performed above as in inner implementation.
                        v.try_push(hash).map(|()| hash).ok()
                    })
                    .flatten()
                    .ok_or(Error::<T>::QueueLimitExceeded)
            })?;

            QueueChanged::<T>::put(true);

            Self::deposit_event(Event::<T>::MessageQueued { message, hash });

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            QueueChanged::<T>::kill();

            T::DbWeight::get().writes(1)
        }

        /// End of the block.
        fn on_finalize(_bn: BlockNumberFor<T>) {
            // Check if queue was changed.
            if !QueueChanged::<T>::get() {
                return;
            }

            // Querying non-empty queue.
            let queue = Queue::<T>::get();

            if queue.is_empty() {
                log::error!("Queue supposed to be non-empty");
                return;
            };

            // Merkle root calculation.
            let root = merkle_tree::merkle_root::<Hasher, _>(queue);

            // Storing new root.
            Self::put_root(root);

            // Depositing event.
            Self::deposit_event(Event::<T>::RootUpdated(root));
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        pub fn get_root() -> Option<H256> {
            let root_with_zeroes = QueueMerkleRoot::<T>::get()?;

            let mut bytes = [0; 32];
            bytes.copy_from_slice(&root_with_zeroes[..32]);

            Some(bytes.into())
        }

        pub fn put_root(root: H256) {
            let mut bytes = [0; 32 + TRAILING_ZEROES];
            bytes[..32].copy_from_slice(root.as_bytes());

            QueueMerkleRoot::<T>::put(bytes)
        }
    }
}
