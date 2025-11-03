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

//! Helpers for working with `sp_core` substrate key pairs.

use crate::error::SignerError;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use hex;
use sp_core::crypto::{CryptoTypeId, Pair as PairTrait, SecretStringError};

/// Trait allowing access to the underlying seed without allocating.
pub trait PairSeed: PairTrait {
    fn pair_seed(pair: &Self) -> Self::Seed {
        let raw = pair.to_raw_vec();
        let mut seed = Self::Seed::default();
        let dst = seed.as_mut();
        let copy_len = core::cmp::min(dst.len(), raw.len());
        dst[..copy_len].copy_from_slice(&raw[..copy_len]);
        seed
    }
}

impl PairSeed for sp_core::sr25519::Pair {}

impl PairSeed for sp_core::ed25519::Pair {
    fn pair_seed(pair: &Self) -> Self::Seed {
        pair.seed()
    }
}

impl PairSeed for sp_core::ecdsa::Pair {
    fn pair_seed(pair: &Self) -> Self::Seed {
        pair.seed()
    }
}

/// Trait providing access to a Substrate crypto key identifier for a pair type.
pub trait HasKeyTypeId {
    const KEY_TYPE_ID: CryptoTypeId;
}

impl HasKeyTypeId for sp_core::sr25519::Pair {
    const KEY_TYPE_ID: CryptoTypeId = sp_core::sr25519::CRYPTO_ID;
}

impl HasKeyTypeId for sp_core::ed25519::Pair {
    const KEY_TYPE_ID: CryptoTypeId = sp_core::ed25519::CRYPTO_ID;
}

impl HasKeyTypeId for sp_core::ecdsa::Pair {
    const KEY_TYPE_ID: CryptoTypeId = sp_core::ecdsa::CRYPTO_ID;
}

/// Returns the printable string for the key type of the provided pair type.
pub fn pair_key_type_string<P: HasKeyTypeId>() -> String {
    crypto_type_id_to_string(P::KEY_TYPE_ID)
}

/// Returns the key type identifier for the provided pair type.
pub fn pair_key_type_id<P: HasKeyTypeId>() -> CryptoTypeId {
    P::KEY_TYPE_ID
}

fn map_secret_err(err: SecretStringError) -> SignerError {
    SignerError::InvalidKey(format!("{err:?}"))
}

/// Construct a pair from a Substrate SURI (secret URI).
pub fn pair_from_suri<P: PairTrait>(suri: &str, password: Option<&str>) -> Result<P, SignerError> {
    P::from_string_with_seed(suri, password)
        .map(|(pair, _)| pair)
        .map_err(map_secret_err)
}

/// Construct a pair from a mnemonic phrase.
pub fn pair_from_phrase<P: PairTrait>(
    phrase: &str,
    password: Option<&str>,
) -> Result<P, SignerError> {
    P::from_phrase(phrase, password)
        .map(|(pair, _)| pair)
        .map_err(map_secret_err)
}

/// Construct a pair from raw seed bytes.
pub fn pair_from_seed_bytes<P: PairTrait>(seed: &[u8]) -> Result<P, SignerError> {
    P::from_seed_slice(seed).map_err(map_secret_err)
}

/// Convert a key type identifier to a printable string.
pub fn crypto_type_id_to_string(id: CryptoTypeId) -> String {
    let bytes = id.0;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let prefix = &bytes[..end];
    core::str::from_utf8(prefix)
        .map(ToString::to_string)
        .unwrap_or_else(|_| hex::encode(bytes))
}

/// Lightweight wrapper around `sp_core::Pair` providing common helpers.
#[derive(Clone)]
pub struct SpPairWrapper<P: PairTrait>(P);

impl<P: PairTrait> SpPairWrapper<P> {
    pub fn new(pair: P) -> Self {
        Self(pair)
    }

    pub fn pair(&self) -> &P {
        &self.0
    }

    pub fn into_inner(self) -> P {
        self.0
    }

    pub fn from_pair_seed(seed: P::Seed) -> Self {
        Self(P::from_seed(&seed))
    }

    pub fn from_seed_bytes(seed: &[u8]) -> Result<Self, SignerError> {
        pair_from_seed_bytes::<P>(seed).map(Self)
    }

    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self, SignerError> {
        pair_from_suri::<P>(suri, password).map(Self)
    }

    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self, SignerError> {
        pair_from_phrase::<P>(phrase, password).map(Self)
    }

    pub fn seed(&self) -> P::Seed
    where
        P: PairSeed,
    {
        P::pair_seed(self.pair())
    }

    pub fn to_raw_vec(&self) -> Vec<u8> {
        self.0.to_raw_vec()
    }
}

#[cfg(feature = "std")]
impl<P: PairTrait> SpPairWrapper<P> {
    pub fn generate() -> Self {
        let (pair, _) = P::generate();
        Self(pair)
    }
}
