// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
use primitive_types::H256;
use sp_std::prelude::*;

pub mod pause;
mod program;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
    };
    use frame_system::{pallet_prelude::*};

    #[pallet::config]
    pub trait Config:
        frame_system::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        // /// Time lock expiration duration for an offchain worker
        // #[pallet::constant]
        // type ExpirationDuration: Get<u64>;

        // /// The maximum number of waitlisted messages to be processed on-chain in one go.
        // #[pallet::constant]
        // type MaxBatchSize: Get<u32>;

        // /// The amount of gas necessary for a trap reply message to be processed.
        // #[pallet::constant]
        // type TrapReplyExistentialGasLimit: Get<u64>;

        // /// The fraction of the collected wait list rent an external submitter will get as a reward
        // #[pallet::constant]
        // type ExternalSubmitterRewardFraction: Get<Perbill>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WaitListRentCollected(u32),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Value not found for a key in storage.
        FailedToGetValueFromStorage,
    }

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type PausedPrograms<T: Config> =
        StorageMap<_, Identity, H256, pause::PausedProgram>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
    }
}
