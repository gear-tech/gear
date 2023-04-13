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
//! `gear_common::{CodeStorage, ProgramStorage}` traits,
//! specifying associated types if needed.
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
//! associated type interface.
//!
//! ## Genesis config
//!
//! The Gear Program Pallet doesn't depend on the `GenesisConfig`.

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::convert::TryInto;

pub use pallet::*;

pub mod migration;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{storage::*, CodeMetadata, Program};
    #[cfg(feature = "debug-mode")]
    use frame_support::storage::PrefixIterator;
    use frame_support::{
        dispatch::EncodeLike, pallet_prelude::*, traits::StorageVersion, StoragePrefixedMap,
    };
    use frame_system::pallet_prelude::*;
    use gear_core::{
        code::InstrumentedCode,
        ids::{CodeId, MessageId, ProgramId},
        memory::{GearPage, PageBuf},
    };
    use sp_runtime::DispatchError;
    use sp_std::prelude::*;

    /// The current storage version.
    const PROGRAM_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    #[pallet::storage_version(PROGRAM_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::error]
    pub enum Error<T> {
        DuplicateItem,
        ItemNotFound,
        NotActiveProgram,
        CannotFindDataForPage,
    }

    impl<T: Config> common::ProgramStorageError for Error<T> {
        fn duplicate_item() -> Self {
            Self::DuplicateItem
        }

        fn item_not_found() -> Self {
            Self::ItemNotFound
        }

        fn not_active_program() -> Self {
            Self::NotActiveProgram
        }

        fn cannot_find_page_data() -> Self {
            Self::CannotFindDataForPage
        }
    }

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type CodeStorage<T: Config> = StorageMap<_, Identity, CodeId, InstrumentedCode>;

    common::wrap_storage_map!(
        storage: CodeStorage,
        name: CodeStorageWrap,
        key: CodeId,
        value: InstrumentedCode
    );

    #[pallet::storage]
    pub(crate) type CodeLenStorage<T: Config> = StorageMap<_, Identity, CodeId, u32>;

    common::wrap_storage_map!(
        storage: CodeLenStorage,
        name: CodeLenStorageWrap,
        key: CodeId,
        value: u32
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
    pub(crate) type MetadataStorage<T: Config> = StorageMap<_, Identity, CodeId, CodeMetadata>;

    common::wrap_storage_map!(
        storage: MetadataStorage,
        name: MetadataStorageWrap,
        key: CodeId,
        value: CodeMetadata
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type ProgramStorage<T: Config> =
        StorageMap<_, Identity, ProgramId, (Program, T::BlockNumber)>;

    common::wrap_storage_map!(
        storage: ProgramStorage,
        name: ProgramStorageWrap,
        key: ProgramId,
        value: (Program, T::BlockNumber)
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type MemoryPageStorage<T: Config> =
        StorageDoubleMap<_, Identity, ProgramId, Identity, GearPage, PageBuf>;

    common::wrap_storage_double_map!(
        storage: MemoryPageStorage,
        name: MemoryPageStorageWrap,
        key1: ProgramId,
        key2: GearPage,
        value: PageBuf
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type WaitingInitStorage<T: Config> =
        StorageMap<_, Identity, ProgramId, Vec<MessageId>>;

    common::wrap_storage_map!(
        storage: WaitingInitStorage,
        name: WaitingInitStorageWrap,
        key: ProgramId,
        value: Vec<MessageId>
    );

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    impl<T: Config> common::CodeStorage for pallet::Pallet<T> {
        type InstrumentedCodeStorage = CodeStorageWrap<T>;
        type InstrumentedLenStorage = CodeLenStorageWrap<T>;
        type MetadataStorage = MetadataStorageWrap<T>;
        type OriginalCodeStorage = OriginalCodeStorageWrap<T>;
    }

    impl<T: Config> common::ProgramStorage for pallet::Pallet<T> {
        type InternalError = Error<T>;
        type Error = DispatchError;
        type BlockNumber = T::BlockNumber;

        type ProgramMap = ProgramStorageWrap<T>;
        type MemoryPageMap = MemoryPageStorageWrap<T>;
        type WaitingInitMap = WaitingInitStorageWrap<T>;

        fn pages_final_prefix() -> [u8; 32] {
            MemoryPageStorage::<T>::final_prefix()
        }
    }

    #[cfg(feature = "debug-mode")]
    impl<T: Config> IterableMap<(ProgramId, (Program, T::BlockNumber))> for pallet::Pallet<T> {
        type DrainIter = PrefixIterator<(ProgramId, (Program, T::BlockNumber))>;
        type Iter = PrefixIterator<(ProgramId, (Program, T::BlockNumber))>;

        fn drain() -> Self::DrainIter {
            ProgramStorage::<T>::drain()
        }

        fn iter() -> Self::Iter {
            ProgramStorage::<T>::iter()
        }
    }

    impl<T: Config> AppendMapStorage<MessageId, ProgramId, Vec<MessageId>>
        for WaitingInitStorageWrap<T>
    {
        fn append<EncodeLikeKey, EncodeLikeItem>(key: EncodeLikeKey, item: EncodeLikeItem)
        where
            EncodeLikeKey: EncodeLike<Self::Key>,
            EncodeLikeItem: EncodeLike<MessageId>,
        {
            WaitingInitStorage::<T>::append(key, item);
        }
    }
}
