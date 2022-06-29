// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! # Gear Scheduler Pallet

#![cfg_attr(not(feature = "std"), no_std)]

// Database migration module.
pub mod migration;

// Runtime mock for running tests.
#[cfg(test)]
mod mock;

// Unit tests module.
#[cfg(test)]
mod tests;

// Public exports from pallet.
pub use pallet::*;

// Gear Scheduler Pallet module.
#[frame_support::pallet]
pub mod pallet {
    pub use frame_support::weights::Weight;

    use frame_support::traits::StorageVersion;
    use sp_std::convert::TryInto;

    /// The current storage version.
    const SCHEDULER_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    // Gear Scheduler Pallet's `Config`.
    #[pallet::config]
    pub trait Config: frame_system::Config {}

    // Gear Scheduler Pallet itself.
    //
    // Uses without storage info to avoid direct access to pallet's
    // storage from outside.
    //
    // Uses `SCHEDULER_STORAGE_VERSION` as current storage version.
    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(SCHEDULER_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Gear Scheduler Pallet error type.
    //
    // Used as inner error type for `Scheduler` implementation.
    #[pallet::error]
    pub enum Error<T> {}
}
