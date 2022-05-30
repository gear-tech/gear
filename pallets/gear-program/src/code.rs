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
use common::{CodeMetadata, CodeStorageError};
use gear_core::{
    code::{CodeAndId, InstrumentedCode, InstrumentedCodeAndId},
    ids::CodeId,
};
use sp_std::vec::Vec;

impl<T: Config> common::CodeStorage for pallet::Pallet<T> {
    fn add_code(code_and_id: CodeAndId, metadata: CodeMetadata) -> Result<(), CodeStorageError> {
        let (code, code_id) = code_and_id.into_parts();
        let (code, original_code) = code.into_parts();
        CodeStorage::<T>::mutate(code_id, |maybe| {
            if maybe.is_some() {
                return Err(CodeStorageError::DuplicateItem);
            }

            OriginalCodeStorage::<T>::insert(code_id, original_code);
            MetadataStorage::<T>::insert(code_id, metadata);

            *maybe = Some(code);
            Ok(())
        })
    }

    fn update_code(code_and_id: InstrumentedCodeAndId) -> bool {
        let (code, code_id) = code_and_id.into_parts();
        CodeStorage::<T>::mutate(code_id, |maybe| match maybe.as_mut() {
            None => false,
            Some(c) => {
                *c = code;
                true
            }
        })
    }

    fn exists(code_id: CodeId) -> bool {
        CodeStorage::<T>::contains_key(code_id)
    }

    fn remove_code(code_id: CodeId) -> bool {
        CodeStorage::<T>::mutate(code_id, |maybe| {
            if maybe.is_none() {
                return false;
            }

            *maybe = None;
            true
        })
    }

    fn get_code(code_id: CodeId) -> Option<InstrumentedCode> {
        CodeStorage::<T>::get(code_id)
    }

    fn get_original_code(code_id: CodeId) -> Option<Vec<u8>> {
        OriginalCodeStorage::<T>::get(code_id)
    }

    fn get_metadata(code_id: CodeId) -> Option<CodeMetadata> {
        MetadataStorage::<T>::get(code_id)
    }
}
