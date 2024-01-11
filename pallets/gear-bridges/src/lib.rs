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

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod weights;

pub use pallet::*;

use frame_support::traits::{Get, StorageVersion};
use gear_core::ids::{BuiltinId, ProgramId};
use pallet_gear_builtin_actor::{BuiltinActor, BuiltinResult, Dispatchable, SimpleBuiltinMessage};
use parity_scale_codec::Encode;
use sp_std::prelude::*;
use weights::WeightInfo;

/// The current storage version.
pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::{BoundedVec, Hooks, MaxEncodedLen, OptionQuery, StorageValue, ValueQuery},
        weights::Weight,
        Parameter,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use sp_runtime::traits::Hash;
    // Bridges pallet.
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Bridges pallet config.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        // Limit of message queue length.
        #[pallet::constant]
        type MaxQueueLength: Get<u32>;
        // Limit of message payload length.
        #[pallet::constant]
        type MaxPayloadLength: Get<u32>;
        // Hasher used to store messages in queue.
        type Hasher: Hash<Output = Self::HashOut>;
        // Hash type used in message queue.
        type HashOut: Parameter + sp_std::hash::Hash + MaxEncodedLen;
        // Weights of calling pallet methods.
        type WeightInfo: WeightInfo;
    }

    // Bridges pallet errors.
    #[pallet::error]
    pub enum Error<T> {
        /// Too much messages in queue.
        QueueOverflow,
        /// Too big message payload.
        MessagePayloadLengthExceeded,
    }

    #[pallet::storage]
    #[pallet::getter(fn queue)]
    type Queue<T: Config> = StorageValue<_, BoundedVec<T::HashOut, T::MaxQueueLength>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pending_bridging)]
    type PendingBridging<T: Config> = StorageValue<_, T::HashOut, OptionQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            Self::process_message_queue(remaining_weight)
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn submit_message(sender: ProgramId, message: &[u8]) -> Result<(), Error<T>> {
            if message.len() > (T::MaxPayloadLength::get() as usize) {
                return Err(Error::MessagePayloadLengthExceeded);
            }

            let hash = T::Hasher::hash(
                &vec![&sender.as_ref(), message]
                    .into_iter()
                    .flatten()
                    .copied()
                    .collect::<Vec<_>>(),
            );
            Queue::<T>::try_append(hash).map_err(|_| Error::QueueOverflow)
        }

        fn process_message_queue(remaining_weight: Weight) -> Weight {
            let db_weight = T::DbWeight::get();
            if !remaining_weight.all_gte(db_weight.reads_writes(1, 2)) {
                return Weight::zero();
            }

            Queue::<T>::mutate(|queue| {
                if !queue.is_empty() {
                    let message = queue.remove(0);
                    PendingBridging::<T>::put(message);
                }
            });

            remaining_weight
        }
    }
}

pub type IncomingMessage = SimpleBuiltinMessage;

impl<T: Config> BuiltinActor<IncomingMessage, u64> for Pallet<T> {
    fn handle(message: &IncomingMessage) -> (BuiltinResult<Vec<u8>>, u64) {
        let result = Self::submit_message(message.source(), &message.payload_bytes());
        let weight = <T as Config>::WeightInfo::handle(T::MaxPayloadLength::get()).ref_time();

        (Ok(result.encode()), weight)
    }

    fn max_gas_cost(builtin_id: BuiltinId) -> u64 {
        <T as Config>::WeightInfo::handle(T::MaxPayloadLength::get()).ref_time()
    }
}
