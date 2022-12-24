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

pub mod migration;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{storage::*, CodeMetadata, Program};
    use frame_support::{pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        code::InstrumentedCode,
        ids::{CodeId, ProgramId},
        memory::{PageBuf, PageNumber},
    };
    use sp_std::prelude::*;

    /// The current storage version.
    const PROGRAM_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    #[pallet::storage_version(PROGRAM_STORAGE_VERSION)]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

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
    pub(crate) type ProgramStorage<T: Config> = StorageMap<_, Identity, ProgramId, Program>;

    common::wrap_storage_map!(
        storage: ProgramStorage,
        name: ProgramStorageWrap,
        key: ProgramId,
        value: Program
    );

    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type MemoryPageStorage<T: Config> =
        StorageDoubleMap<_, Identity, ProgramId, Identity, PageNumber, PageBuf>;

    common::wrap_storage_double_map!(
        storage: MemoryPageStorage,
        name: MemoryPageStorageWrap,
        key1: ProgramId,
        key2: PageNumber,
        value: PageBuf
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
        type ProgramMap = ProgramStorageWrap<T>;
        type MemoryPageMap = MemoryPageStorageWrap<T>;
    }

    #[cfg(feature = "debug-mode")]
    impl<T: Config> IterableMap<(ProgramId, Program)> for pallet::Pallet<T> {
        type DrainIter = frame_support::storage::PrefixIterator<(ProgramId, Program)>;
        type Iter = frame_support::storage::PrefixIterator<(ProgramId, Program)>;

        fn drain() -> Self::DrainIter {
            ProgramStorage::<T>::drain()
        }

        fn iter() -> Self::Iter {
            ProgramStorage::<T>::iter()
        }
    }
}
