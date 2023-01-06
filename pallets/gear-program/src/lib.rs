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
    use codec::EncodeLike;
    use common::{storage::*, CodeMetadata, Program};
    #[cfg(feature = "debug-mode")]
    use frame_support::storage::PrefixIterator;
    use frame_support::{pallet_prelude::*, traits::StorageVersion, StoragePrefixedMap};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        code::InstrumentedCode,
        ids::{CodeId, MessageId, ProgramId},
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

    impl<Runtime: Config> common::ProgramStorage for pallet::Pallet<Runtime> {
        type ProgramMap = ProgramStorageWrap<Runtime>;
        type MemoryPageMap = MemoryPageStorageWrap<Runtime>;
        type WaitingInitMap = WaitingInitStorageWrap<Runtime>;

        fn pages_final_prefix() -> [u8; 32] {
            MemoryPageStorage::<Runtime>::final_prefix()
        }
    }

    #[cfg(feature = "debug-mode")]
    impl<Runtime: Config> IterableMap<(ProgramId, Program)> for pallet::Pallet<Runtime> {
        type DrainIter = PrefixIterator<(ProgramId, Program)>;
        type Iter = PrefixIterator<(ProgramId, Program)>;

        fn drain() -> Self::DrainIter {
            ProgramStorage::<Runtime>::drain()
        }

        fn iter() -> Self::Iter {
            ProgramStorage::<Runtime>::iter()
        }
    }

    impl<Runtime: Config> AppendMapStorage<MessageId, ProgramId, Vec<MessageId>>
        for WaitingInitStorageWrap<Runtime>
    {
        fn append<EncodeLikeKey, EncodeLikeItem>(key: EncodeLikeKey, item: EncodeLikeItem)
        where
            EncodeLikeKey: EncodeLike<Self::Key>,
            EncodeLikeItem: EncodeLike<MessageId>,
        {
            WaitingInitStorage::<Runtime>::append(key, item);
        }
    }
}
