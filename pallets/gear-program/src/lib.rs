// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

//! # Gear Program Pallet
//!
//! The Gear Program Pallet provides functionality for storing programs
//! and binary codes.
//!
//! - [`Config`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! The Gear Program Pallet's main aim is to separate programs and binary codes storages out
//! of Gear's execution logic and provide soft functionality to manage them.
//!
//! The Gear Program Pallet provides functions for:
//! - Add/remove/check existence for binary codes;
//! - Get original binary code, instrumented binary code and associated metadata;
//! - Update instrumented binary code in the storage;
//! - Add/remove/check existence for programs;
//! - Get program data;
//! - Update program in the storage;
//! - Work with program memory pages and messages for uninitialized programs.
//!
//! ## Interface
//!
//! The Gear Program Pallet implements `gear_common::{CodeStorage, ProgramStorage}` traits
//! and shouldn't contain any other functionality, except this trait declares.
//!
//! ## Usage
//!
//! How to use the functionality from the Gear Program Pallet:
//!
//! 1. Implement the pallet `Config` for your runtime.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl pallet_gear_program::Config for Runtime {}
//!
//! // ... //
//! ```
//!
//! 2. Provide associated type for your pallet's `Config`, which implements
//!    `gear_common::{CodeStorage, ProgramStorage}` traits,
//!    specifying associated types if needed.
//!
//! ```ignore
//! // `some_pallet/src/lib.rs`
//! // ... //
//!
//! use gear_common::{CodeStorage, ProgramStorage};
//!
//! #[pallet::config]
//! pub trait Config: frame_system::Config {
//!     // .. //
//!
//!     type CodeStorage: CodeStorage;
//!
//!     type ProgramStorage: ProgramStorage;
//!
//!     // .. //
//! }
//! ```
//!
//! 3. Declare Gear Program Pallet in your `construct_runtime!` macro.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! construct_runtime!(
//!     pub enum Runtime
//!         where // ... //
//!     {
//!         // ... //
//!
//!         GearProgram: pallet_gear_program,
//!
//!         // ... //
//!     }
//! );
//!
//! // ... //
//! ```
//!
//! 4. Set `GearProgram` as your pallet `Config`'s `{CodeStorage, ProgramStorage}` types.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl some_pallet::Config for Runtime {
//!     // ... //
//!
//!     type CodeStorage = GearProgram;
//!
//!     type ProgramStorage = GearProgram;
//!
//!     // ... //
//! }
//!
//! // ... //
//! ```
//!
//! 5. Work with Gear Program Pallet in your pallet with provided
//!    associated type interface.
//!
//! ## Genesis config
//!
//! The Gear Program Pallet doesn't depend on the `GenesisConfig`.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]
#![allow(clippy::manual_inspect)]

extern crate alloc;

use sp_std::{convert::TryInto, prelude::*};

pub use pallet::*;

#[cfg(test)]
mod mock;

pub mod pallet_tests;

pub mod migrations;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{scheduler::*, storage::*};
    use frame_support::{
        pallet_prelude::*,
        storage::{Key, PrefixIterator},
        traits::StorageVersion,
        StoragePrefixedMap,
    };
    use frame_system::pallet_prelude::*;
    use gear_core::{
        code::{CodeMetadata, InstrumentedCode},
        ids::{ActorId, CodeId},
        memory::PageBuf,
        pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
        program::{MemoryInfix, Program},
        tasks::VaraScheduledTask,
    };
    use sp_runtime::DispatchError;

    /// The current storage version.
    pub(crate) const PROGRAM_STORAGE_VERSION: StorageVersion = StorageVersion::new(13);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Scheduler.
        type Scheduler: Scheduler<
            BlockNumber = BlockNumberFor<Self>,
            Task = VaraScheduledTask<Self::AccountId>,
        >;

        /// Custom block number tracker.
        type CurrentBlockNumber: Get<BlockNumberFor<Self>>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(PROGRAM_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::error]
    pub enum Error<T> {
        DuplicateItem,
        ProgramNotFound,
        NotActiveProgram,
        CannotFindDataForPage,
        ProgramCodeNotFound,
    }

    impl<T: Config> common::ProgramStorageError for Error<T> {
        fn duplicate_item() -> Self {
            Self::DuplicateItem
        }

        fn program_not_found() -> Self {
            Self::ProgramNotFound
        }

        fn not_active_program() -> Self {
            Self::NotActiveProgram
        }

        fn cannot_find_page_data() -> Self {
            Self::CannotFindDataForPage
        }

        fn program_code_not_found() -> Self {
            Self::ProgramCodeNotFound
        }
    }

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type InstrumentedCodeStorage<T: Config> =
        StorageMap<_, Identity, CodeId, InstrumentedCode>;

    common::wrap_storage_map!(
        storage: InstrumentedCodeStorage,
        name: InstrumentedCodeStorageWrap,
        key: CodeId,
        value: InstrumentedCode
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type OriginalCodeStorage<T: Config> = StorageMap<_, Identity, CodeId, Vec<u8>>;

    common::wrap_storage_map!(
        storage: OriginalCodeStorage,
        name: OriginalCodeStorageWrap,
        key: CodeId,
        value: Vec<u8>
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type CodeMetadataStorage<T: Config> = StorageMap<_, Identity, CodeId, CodeMetadata>;

    common::wrap_storage_map!(
        storage: CodeMetadataStorage,
        name: CodeMetadataStorageWrap,
        key: CodeId,
        value: CodeMetadata
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type AllocationsStorage<T: Config> =
        StorageMap<_, Identity, ActorId, IntervalsTree<WasmPage>>;

    common::wrap_storage_map!(
        storage: AllocationsStorage,
        name: AllocationsStorageWrap,
        key: ActorId,
        value: IntervalsTree<WasmPage>
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type ProgramStorage<T: Config> =
        StorageMap<_, Identity, ActorId, Program<BlockNumberFor<T>>>;

    common::wrap_storage_map!(
        storage: ProgramStorage,
        name: ProgramStorageWrap,
        key: ActorId,
        value: Program<BlockNumberFor<T>>
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type MemoryPages<T: Config> = StorageNMap<
        _,
        (
            Key<Identity, ActorId>,
            Key<Identity, MemoryInfix>,
            Key<Identity, GearPage>,
        ),
        PageBuf,
    >;

    common::wrap_storage_triple_map!(
        storage: MemoryPages,
        name: MemoryPageStorageWrap,
        key1: ActorId,
        key2: MemoryInfix,
        key3: GearPage,
        value: PageBuf
    );

    impl<T: Config> common::CodeStorage for pallet::Pallet<T> {
        type InstrumentedCodeMap = InstrumentedCodeStorageWrap<T>;
        type OriginalCodeMap = OriginalCodeStorageWrap<T>;
        type CodeMetadataMap = CodeMetadataStorageWrap<T>;
    }

    impl<T: Config> common::ProgramStorage for pallet::Pallet<T> {
        type InternalError = Error<T>;
        type Error = DispatchError;
        type BlockNumber = BlockNumberFor<T>;
        type AccountId = T::AccountId;

        type AllocationsMap = AllocationsStorageWrap<T>;
        type ProgramMap = ProgramStorageWrap<T>;
        type MemoryPageMap = MemoryPageStorageWrap<T>;

        fn pages_final_prefix() -> [u8; 32] {
            MemoryPages::<T>::final_prefix()
        }
    }

    impl<T: Config> IterableMap<(ActorId, Program<BlockNumberFor<T>>)> for pallet::Pallet<T> {
        type DrainIter = PrefixIterator<(ActorId, Program<BlockNumberFor<T>>)>;
        type Iter = PrefixIterator<(ActorId, Program<BlockNumberFor<T>>)>;

        fn drain() -> Self::DrainIter {
            ProgramStorage::<T>::drain()
        }

        fn iter() -> Self::Iter {
            ProgramStorage::<T>::iter()
        }
    }
}
