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
pub use weights::WeightInfo;

pub mod migrations;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, storage::*, CodeStorage, ProgramStorage};
    use core::fmt;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::ActorId,
        memory::PageBuf,
        message::{StoredDelayedDispatch, StoredDispatch},
        pages::{GearPage, WasmPagesAmount},
        program::Program,
    };
    use primitive_types::H256;
    use scale_info::TypeInfo;
    use sp_std::{
        collections::{btree_map::BTreeMap, btree_set::BTreeSet},
        convert::TryInto,
        prelude::*,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>
            + TryInto<Event<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

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

    #[pallet::event]
    pub enum Event<T: Config> {
        DebugMode(bool),
        /// A snapshot of the debug data: programs and message queue ('debug mode' only)
        DebugDataSnapshot(DebugData),
    }

    // GearSupport pallet error.
    #[pallet::error]
    pub enum Error<T> {}

    /// Program debug info.
    #[derive(Encode, Decode, Clone, Default, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
    pub struct ProgramInfo {
        pub static_pages: WasmPagesAmount,
        pub persistent_pages: BTreeMap<GearPage, PageBuf>,
        pub code_hash: H256,
    }

    impl fmt::Debug for ProgramInfo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ProgramInfo")
                .field("static_pages", &self.static_pages)
                .field(
                    "persistent_pages",
                    &self
                        .persistent_pages
                        .iter()
                        .map(|(page, data)|
                        // Prints only bytes which is not zero
                        (
                            *page,
                            data.iter()
                                .enumerate()
                                .filter(|(_, val)| **val != 0)
                                .map(|(idx, val)| (idx, *val))
                                .collect::<BTreeMap<_, _>>(),
                        ))
                        .collect::<BTreeMap<_, _>>(),
                )
                .field("code_hash", &self.code_hash)
                .finish()
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, TypeInfo, Debug)]
    pub enum ProgramState {
        Active(ProgramInfo),
        Terminated,
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, TypeInfo, Debug)]
    pub struct ProgramDetails {
        pub id: ActorId,
        pub state: ProgramState,
    }

    #[derive(Debug, Encode, Decode, Clone, Default, PartialEq, Eq, TypeInfo)]
    pub struct DebugData {
        pub dispatch_queue: Vec<StoredDispatch>,
        pub programs: BTreeSet<ProgramDetails>,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            Weight::zero()
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }
}
