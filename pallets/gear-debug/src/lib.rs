// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
#![allow(clippy::manual_inspect)]
#![allow(clippy::useless_conversion)]

extern crate alloc;

pub use pallet::*;

pub mod migrations;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, storage::*, CodeStorage, ProgramStorage};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::ActorId,
        message::{StoredDelayedDispatch, StoredDispatch},
        program::Program,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Storage with codes for programs.
        type CodeStorage: CodeStorage;

        type Messenger: Messenger<
            QueuedDispatch = StoredDispatch,
            DelayedDispatch = StoredDelayedDispatch,
        >;

        type ProgramStorage: ProgramStorage + IterableMap<(ActorId, Program<BlockNumberFor<Self>>)>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    // GearSupport pallet error.
    #[pallet::error]
    pub enum Error<T> {}
}
