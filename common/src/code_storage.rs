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

use super::*;
use crate::storage::MapStorage;
use gear_core::code::{CodeAndId, InstrumentedCode, InstrumentedCodeAndId};

#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Code already exists in storage.
    DuplicateItem,
}

/// Trait to work with program binary codes in a storage.
pub trait CodeStorage {
    type InstrumentedCodeStorage: MapStorage<Key = CodeId, Value = InstrumentedCode>;
    type InstrumentedLenStorage: MapStorage<Key = CodeId, Value = u32>;
    type OriginalCodeStorage: MapStorage<Key = CodeId, Value = Vec<u8>>;
    type MetadataStorage: MapStorage<Key = CodeId, Value = CodeMetadata>;

    /// Attempt to remove all items from all the associated maps.
    fn reset() {
        Self::MetadataStorage::clear();
        Self::OriginalCodeStorage::clear();
        Self::InstrumentedLenStorage::clear();
        Self::InstrumentedCodeStorage::clear();
    }

    fn add_code(code_and_id: CodeAndId, metadata: CodeMetadata) -> Result<(), Error> {
        let (code, code_id) = code_and_id.into_parts();
        let (code, original_code) = code.into_parts();

        Self::InstrumentedCodeStorage::mutate(code_id, |maybe| {
            if maybe.is_some() {
                return Err(CodeStorageError::DuplicateItem);
            }

            Self::InstrumentedLenStorage::insert(code_id, code.code().len() as u32);
            Self::OriginalCodeStorage::insert(code_id, original_code);
            Self::MetadataStorage::insert(code_id, metadata);

            *maybe = Some(code);
            Ok(())
        })
    }

    /// Update the corresponding code in the storage.
    fn update_code(code_and_id: InstrumentedCodeAndId) {
        let (code, code_id) = code_and_id.into_parts();

        Self::InstrumentedLenStorage::insert(code_id, code.code().len() as u32);
        Self::InstrumentedCodeStorage::insert(code_id, code);
    }

    fn exists(code_id: CodeId) -> bool {
        Self::InstrumentedCodeStorage::contains_key(&code_id)
    }

    /// Returns true if the code associated with given id was removed.
    ///
    /// If there is no code for the given id then false is returned.
    fn remove_code(code_id: CodeId) -> bool {
        Self::InstrumentedCodeStorage::mutate(code_id, |maybe| {
            if maybe.is_none() {
                return false;
            }

            Self::InstrumentedLenStorage::remove(code_id);
            Self::OriginalCodeStorage::remove(code_id);
            Self::MetadataStorage::remove(code_id);

            *maybe = None;
            true
        })
    }

    fn get_code(code_id: CodeId) -> Option<InstrumentedCode> {
        Self::InstrumentedCodeStorage::get(&code_id)
    }

    fn get_code_len(code_id: CodeId) -> Option<u32> {
        Self::InstrumentedLenStorage::get(&code_id)
    }

    fn get_original_code(code_id: CodeId) -> Option<Vec<u8>> {
        Self::OriginalCodeStorage::get(&code_id)
    }

    fn get_metadata(code_id: CodeId) -> Option<CodeMetadata> {
        Self::MetadataStorage::get(&code_id)
    }
}
