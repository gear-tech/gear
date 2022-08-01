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

use gear_core::code::{CodeAndId, InstrumentedCode, InstrumentedCodeAndId};

#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Code already exists in storage.
    DuplicateItem,
}

/// Trait to work with program binary codes in a storage.
pub trait CodeStorage {
    fn add_code(code_and_id: CodeAndId, metadata: CodeMetadata) -> Result<(), Error>;
    /// Returns true if the corresponding code in the storage
    /// and it was updated successfully.
    fn update_code(code_and_id: InstrumentedCodeAndId) -> bool;
    fn exists(code_id: CodeId) -> bool;
    /// Returns true if the code associated with given id was removed.
    ///
    /// If there is no code for the given id then false is returned.
    fn remove_code(code_id: CodeId) -> bool;
    fn get_code(code_id: CodeId) -> Option<InstrumentedCode>;
    fn get_original_code(code_id: CodeId) -> Option<Vec<u8>>;
    fn get_metadata(code_id: CodeId) -> Option<CodeMetadata>;
}
