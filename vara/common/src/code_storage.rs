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

use super::*;
use crate::storage::MapStorage;
use gear_core::code::{CodeAndId, CodeMetadata, InstrumentedCode, InstrumentedCodeAndMetadata};

#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Code already exists in storage.
    DuplicateItem,
}

/// Trait to work with program binary codes in a storage.
pub trait CodeStorage {
    type InstrumentedCodeMap: MapStorage<Key = CodeId, Value = InstrumentedCode>;
    type OriginalCodeMap: MapStorage<Key = CodeId, Value = Vec<u8>>;
    type CodeMetadataMap: MapStorage<Key = CodeId, Value = CodeMetadata>;

    /// Attempt to remove all items from all the associated maps.
    fn reset() {
        Self::CodeMetadataMap::clear();
        Self::OriginalCodeMap::clear();
        Self::InstrumentedCodeMap::clear();
    }

    /// Add the code to the storage.
    fn add_code(code_and_id: CodeAndId) -> Result<(), Error> {
        let (code, code_id) = code_and_id.into_parts();
        let (original_code, instrumented_code, code_metadata) = code.into_parts();

        Self::OriginalCodeMap::mutate(code_id, |maybe| {
            if maybe.is_some() {
                return Err(CodeStorageError::DuplicateItem);
            }

            Self::InstrumentedCodeMap::insert(code_id, instrumented_code);
            Self::CodeMetadataMap::insert(code_id, code_metadata);

            *maybe = Some(original_code);
            Ok(())
        })
    }

    /// Update the corresponding code and metadata in the storage.
    fn update_instrumented_code_and_metadata(
        code_id: CodeId,
        instrumented_code_and_metadata: InstrumentedCodeAndMetadata,
    ) {
        Self::InstrumentedCodeMap::insert(
            code_id,
            instrumented_code_and_metadata.instrumented_code,
        );
        Self::CodeMetadataMap::insert(code_id, instrumented_code_and_metadata.metadata);
    }

    /// Update the corresponding metadata in the storage.
    fn update_code_metadata(code_id: CodeId, metadata: CodeMetadata) {
        Self::CodeMetadataMap::insert(code_id, metadata);
    }

    /// Returns true if the original code associated with given id exists.
    fn original_code_exists(code_id: CodeId) -> bool {
        Self::OriginalCodeMap::contains_key(&code_id)
    }

    /// Returns true if the instrumented code associated with given id exists.
    fn instrumented_code_exists(code_id: CodeId) -> bool {
        Self::InstrumentedCodeMap::contains_key(&code_id)
    }

    /// Returns true if the code associated with given id was removed.
    ///
    /// If there is no code for the given id then false is returned.
    fn remove_code(code_id: CodeId) -> bool {
        Self::OriginalCodeMap::mutate(code_id, |maybe| {
            if maybe.is_none() {
                return false;
            }

            Self::InstrumentedCodeMap::remove(code_id);
            Self::CodeMetadataMap::remove(code_id);

            *maybe = None;
            true
        })
    }

    fn get_instrumented_code(code_id: CodeId) -> Option<InstrumentedCode> {
        Self::InstrumentedCodeMap::get(&code_id)
    }

    fn get_original_code(code_id: CodeId) -> Option<Vec<u8>> {
        Self::OriginalCodeMap::get(&code_id)
    }

    fn get_code_metadata(code_id: CodeId) -> Option<CodeMetadata> {
        Self::CodeMetadataMap::get(&code_id)
    }
}
