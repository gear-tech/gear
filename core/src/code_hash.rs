// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Module for code hashes.

use crate::{checked_code::CheckedCode, program::InstrumentedCode};
use codec::{Decode, Encode};
use core::convert::TryFrom;

const HASH_LENGTH: usize = 32;

/// Blake2b hash of the program code.
#[derive(Clone, Copy, Debug, Decode, Encode, Ord, PartialOrd, Eq, PartialEq)]
pub struct CodeHash([u8; HASH_LENGTH]);

impl CodeHash {
    /// Instantiates [`CodeHash`] by computing blake2b hash of the program `code`.
    pub fn generate(code: &[u8]) -> Self {
        let blake2b_hash = blake2_rfc::blake2b::blake2b(HASH_LENGTH, &[], code);
        Self::from_slice_unchecked(blake2b_hash.as_bytes())
            .expect("Blake2bResult has the specified len")
    }

    /// Get inner (32 bytes) array representation
    pub fn inner(&self) -> [u8; HASH_LENGTH] {
        self.0
    }

    /// Create new `CodeHash` bytes. Returns `None` if the provided slice has wrong length.
    pub fn from_slice_unchecked(s: &[u8]) -> Option<Self> {
        <[u8; HASH_LENGTH]>::try_from(s).map(Into::into).ok()
    }
}

impl From<[u8; HASH_LENGTH]> for CodeHash {
    fn from(data: [u8; HASH_LENGTH]) -> Self {
        CodeHash(data)
    }
}

/// Contains checked code for a program and the hash for it.
pub struct CheckedCodeWithHash(CheckedCode, CodeHash);

impl CheckedCodeWithHash {
    /// Creates new instance from the provided code.
    pub fn new(code: CheckedCode) -> Self {
        let hash = CodeHash::generate(code.code());
        Self(code, hash)
    }

    /// Returns reference to the checked code.
    pub fn code(&self) -> &CheckedCode {
        &self.0
    }

    /// Returns reference to the code hash.
    pub fn hash(&self) -> &CodeHash {
        &self.1
    }

    /// Decomposes this instance.
    pub fn into_parts(self) -> (CheckedCode, CodeHash) {
        (self.0, self.1)
    }
}

/// Contains instumented code for a program and the hash for it.
pub struct InstrumentedCodeWithHash(InstrumentedCode, CodeHash);

impl InstrumentedCodeWithHash {
    /// Creates new instance from the provided code.
    pub fn new(code: InstrumentedCode, hash: CodeHash) -> Self {
        Self(code, hash)
    }

    /// Returns reference to the checked code.
    pub fn code(&self) -> &InstrumentedCode {
        &self.0
    }

    /// Returns reference to the code hash.
    pub fn hash(&self) -> &CodeHash {
        &self.1
    }

    /// Decomposes this instance.
    pub fn into_parts(self) -> (InstrumentedCode, CodeHash) {
        (self.0, self.1)
    }
}
