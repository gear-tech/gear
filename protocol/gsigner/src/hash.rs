// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Lightweight hashing helpers shared across schemes.

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
