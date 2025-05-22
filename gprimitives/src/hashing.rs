// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Commonly used hashing functions.

use blake2::{digest::typenum::U32, Blake2b, Digest};

/// BLAKE2b-256 hasher state.
pub type Blake2b256 = Blake2b<U32>;

/// Hashes a given bytes into a 32-byte array using the BLAKE2b-256 hash function.
///
/// # SAFETY
/// Do not adjust the hash function, as the IDs generation is sensitive to it.
pub fn hash(data: &[u8]) -> [u8; 32] {
    hash_array([data])
}

/// Concatenates and hashes a given bytes into a 32-byte array using the BLAKE2b-256 hash function.
///
/// # SAFETY
/// Do not adjust the hash function, as the IDs generation is sensitive to it.
pub fn hash_array<T: AsRef<[u8]>, const N: usize>(array: [T; N]) -> [u8; 32] {
    let mut ctx = Blake2b256::new();
    for data in array {
        ctx.update(data);
    }
    ctx.finalize().into()
}
