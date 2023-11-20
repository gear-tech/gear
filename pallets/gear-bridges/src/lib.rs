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

//! # Pallet storing messages sent over bridge.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

use frame_support::traits::StorageVersion;

#[macro_export]
macro_rules! impl_config {
    ($runtime:ty) => {
        impl pallet_gear_bridges::Config for $runtime {}
    };
}

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

pub(crate) type HasherOf<T> = <T as frame_system::Config>::Hashing;
pub(crate) type HashOf<T> = <HasherOf<T> as sp_runtime::traits::Hash>::Output;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::{BoundedVec, Hooks, OptionQuery, StorageValue, ValueQuery},
        traits::Get,
        weights::Weight,
    };
    use frame_system::pallet_prelude::BlockNumberFor;

    // Bridges pallet.
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Bridges pallet config.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        // Limit of message queue length.
        type MaxQueueLength: Get<u32>;
    }

    // Bridges pallet errors.
    #[pallet::error]
    pub enum Error<T> {
        /// Too much messages in queue.
        QueueOverflow,
    }

    #[pallet::storage]
    #[pallet::getter(fn queue)]
    type Queue<T: Config> = StorageValue<_, BoundedVec<HashOf<T>, T::MaxQueueLength>, ValueQuery>;

    #[pallet::storage]
    type PendingBridging<T: Config> = StorageValue<_, HashOf<T>, OptionQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            Self::process_message_queue(remaining_weight)
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn submit_message(message: &[u8]) -> Result<(), Error<T>> {
            let hash = <HasherOf<T> as sp_runtime::traits::Hash>::hash(message);
            Queue::<T>::try_append(hash).map_err(|_| Error::QueueOverflow)
        }

        fn process_message_queue(remaining_weight: Weight) -> Weight {
            let db_weight = T::DbWeight::get();
            if !remaining_weight.all_gte(db_weight.reads_writes(1, 2)) {
                return Weight::zero();
            }

            Queue::<T>::mutate(|queue| {
                if !queue.is_empty() {
                    let message = queue.remove(queue.len() - 1);
                    PendingBridging::<T>::put(message);
                }
            });

            remaining_weight
        }
    }
}
