// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Lightweight hashing helpers shared across schemes.

use crate::ToDigest;
use core::marker::PhantomData;
use parity_scale_codec::{Decode, Encode};
use sha3::{Digest as _, Keccak256};


/// Compute the Keccak-256 hash of a byte slice.
#[inline]
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Compute the Keccak-256 hash of several concatenated byte slices without
/// allocating intermediate buffers.
#[inline]
pub fn keccak256_iter<'a, I>(parts: I) -> [u8; 32]
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut hasher = Keccak256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Representing the EIP-191 hash standard.
#[derive(Debug, PartialEq, Eq, Encode, Decode)]
pub struct Eip191Hash<T> {
    hash: [u8; 32],
    _phantom: PhantomData<T>,
}

impl<T> Copy for Eip191Hash<T> {}

impl<T> Clone for Eip191Hash<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Eip191Hash<T> {
    pub fn inner(&self) -> &[u8; 32] {
        &self.hash
    }
}

impl<T> Eip191Hash<T>
where
    T: ToDigest,
{
    /// Constructs the [`Eip191Hash`] from [`Digest`].
    pub fn new(value: &T) -> Eip191Hash<T> {
        let digest = value.to_digest();
        let mut hasher = Keccak256::new();

        hasher.update(b"\x19Ethereum Signed Message:\n");
        hasher.update(b"32");
        hasher.update(digest.0.as_ref());

        Eip191Hash {
            hash: hasher.finalize().into(),
            _phantom: PhantomData,
        }
    }
}
