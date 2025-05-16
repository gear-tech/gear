// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Keccak256 digest type.
//!
//! Implements `ToDigest` hashing for ethexe common types.

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

/// Common digest type for the ethexe.
/// Presently, it is represented as 32-byte Keccak256 hash.
/// The `ToDigest` trait is implemented for various types to facilitate hashing and signing.
#[derive(
    Clone,
    Copy,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    Encode,
    Decode,
    derive_more::From,
    derive_more::Into,
    derive_more::Debug,
    derive_more::Display,
)]
#[repr(transparent)]
#[debug("0x{}", hex::encode(self.0))]
#[display("0x{}", hex::encode(self.0))]
pub struct Digest([u8; 32]);

impl<T> FromIterator<T> for Digest
where
    Digest: From<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut hasher = sha3::Keccak256::new();
        for item in iter {
            hasher.update(Digest::from(item).as_ref());
        }
        Digest(hasher.finalize().into())
    }
}

/// Trait for hashing types into a Digest using Keccak256.
pub trait ToDigest {
    fn to_digest(&self) -> Digest {
        let mut hasher = sha3::Keccak256::new();
        self.update_hasher(&mut hasher);
        Digest(hasher.finalize().into())
    }

    fn update_hasher(&self, hasher: &mut sha3::Keccak256);
}

impl<T: ToDigest + ?Sized> ToDigest for &T {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        (*self).update_hasher(hasher);
    }
}

impl<T: ToDigest> From<T> for Digest {
    fn from(item: T) -> Self {
        item.to_digest()
    }
}

impl<T: ToDigest> ToDigest for [T] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        for item in self {
            hasher.update(item.to_digest().as_ref());
        }
    }
}

impl<T: ToDigest> ToDigest for Vec<T> {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        self.as_slice().update_hasher(hasher);
    }
}

impl ToDigest for [u8] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self);
    }
}

impl ToDigest for Vec<u8> {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.as_slice());
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
