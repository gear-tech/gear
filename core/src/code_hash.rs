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

use crate::checked_code::CheckedCode;
use codec::{Decode, Encode};

/// Blake2b hash of the program code.
#[derive(Clone, Copy, Debug, Decode, Encode, Ord, PartialOrd, Eq, PartialEq)]
pub struct CodeHash([u8; 32]);

impl CodeHash {
    /// Instantiates [`CodeHash`] by computing blake2b hash of the program `code`.
    pub fn generate(code: &[u8]) -> Self {
        let blake2b_hash = blake2_rfc::blake2b::blake2b(32, &[], code);
        Self::from_slice(blake2b_hash.as_bytes())
    }

    /// Get inner (32 bytes) array representation
    pub fn inner(&self) -> [u8; 32] {
        self.0
    }

    /// Create new `CodeHash` bytes.
    ///
    /// Will panic if slice is not 32 bytes length.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("Slice is not 32 bytes length")
        };
        let mut id = CodeHash([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
    }
}

impl From<[u8; 32]> for CodeHash {
    fn from(data: [u8; 32]) -> Self {
        CodeHash(data)
    }
}

/// Contains checked code for a program and the hash for it.
pub struct CheckedCodeHash(CheckedCode, CodeHash);

impl CheckedCodeHash {
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
    pub fn into_code_hash(self) -> (CheckedCode, CodeHash) {
        (self.0, self.1)
    }
}
