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

//! secp256k1 key types backed by `sp_core` primitives.

use super::Address;
use crate::{
    error::SignerError,
    substrate_utils::{PairSeed, SpPairWrapper},
    traits::SeedableKey,
    utils::decode_hex_to_array,
};
#[cfg(feature = "serde")]
use alloc::vec::Vec;
use alloc::{
    format,
    string::{String, ToString},
};
use core::{convert::TryInto, fmt, str::FromStr};
use k256::ecdsa::VerifyingKey;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{
    crypto::{ByteArray, Pair as PairTrait},
    ecdsa::{Pair as SpPair, Public as SpPublic},
};

/// Seed type alias matching `sp_core::ecdsa::Pair`.
pub type Seed = <SpPair as PairTrait>::Seed;

/// secp256k1 private key (ECDSA) backed by `sp_core::ecdsa::Pair`.
#[derive(Clone)]
pub struct PrivateKey(SpPairWrapper<SpPair>);

impl PrivateKey {
    /// Generate a new random private key.
    #[cfg(feature = "std")]
    pub fn random() -> Self {
        Self(SpPairWrapper::generate())
    }

    /// Construct a private key from a Substrate SURI.
    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self, SignerError> {
        SpPairWrapper::from_suri(suri, password).map(Self)
    }

    /// Construct a private key from a mnemonic phrase.
    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self, SignerError> {
        SpPairWrapper::from_phrase(phrase, password).map(Self)
    }

    /// Construct from the underlying Substrate seed type.
    pub fn from_pair_seed(seed: Seed) -> Self {
        Self(SpPairWrapper::from_pair_seed(seed))
    }

    /// Construct from a raw 32-byte secret seed.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self, SignerError> {
        SpPairWrapper::from_seed_bytes(&seed).map(Self)
    }

    /// Return the raw secret seed bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        let seed = self.seed();
        seed.as_ref()
            .try_into()
            .expect("ecdsa seed has fixed length")
    }

    /// Get the associated public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.pair().public())
    }

    /// Access the underlying Substrate pair.
    pub fn as_pair(&self) -> &SpPair {
        self.0.pair()
    }

    /// Return the underlying seed type.
    pub fn seed(&self) -> Seed {
        PairSeed::pair_seed(self.as_pair())
    }

    /// Construct from an existing Substrate pair.
    pub(crate) fn from_pair(pair: SpPair) -> Self {
        Self(SpPairWrapper::new(pair))
    }
}

impl From<SpPair> for PrivateKey {
    fn from(pair: SpPair) -> Self {
        Self(SpPairWrapper::new(pair))
    }
}

impl From<PrivateKey> for SpPair {
    fn from(key: PrivateKey) -> Self {
        key.0.into_inner()
    }
}

impl SeedableKey for PrivateKey {
    type Seed = Seed;

    fn from_seed(seed: Self::Seed) -> crate::error::Result<Self> {
        Ok(PrivateKey::from_pair_seed(seed))
    }

    fn seed(&self) -> Self::Seed {
        PrivateKey::seed(self)
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.seed().as_ref()))
    }
}

impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.seed().as_ref()))
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for PrivateKey {}

impl FromStr for PrivateKey {
    type Err = SignerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes =
            decode_hex_to_array::<32>(s).map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Self::from_seed(bytes)
    }
}

#[cfg(feature = "serde")]
impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.seed().as_ref())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid private key length"));
        }
        let mut seed = Seed::default();
        seed.as_mut().copy_from_slice(&bytes);
        Ok(PrivateKey::from_pair_seed(seed))
    }
}

/// secp256k1 public key backed by `sp_core::ecdsa::Public` (compressed form).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey(SpPublic);

impl PublicKey {
    /// Create public key from the private key.
    pub fn from_private(private_key: &PrivateKey) -> Self {
        private_key.public_key()
    }

    /// Construct from compressed public key bytes.
    pub fn from_bytes(bytes: [u8; 33]) -> Result<Self, SignerError> {
        SpPublic::try_from(&bytes[..])
            .map(Self)
            .map_err(|_| SignerError::InvalidKey("Invalid compressed public key".into()))
    }

    /// Public key as compressed bytes.
    pub fn to_bytes(self) -> [u8; 33] {
        self.0
            .as_slice()
            .try_into()
            .expect("compressed key has fixed length")
    }

    /// Public key hex string (compressed form).
    pub fn to_hex(self) -> String {
        hex::encode(self.0.as_slice())
    }

    /// Convert public key to Ethereum address.
    pub fn to_address(self) -> Address {
        Address::from(self)
    }

    /// Convert public key to uncompressed bytes (without prefix).
    pub fn to_uncompressed(self) -> [u8; 64] {
        VerifyingKey::from_sec1_bytes(self.0.as_ref())
            .expect("compressed key is always valid")
            .to_encoded_point(false)
            .as_bytes()[1..]
            .try_into()
            .expect("uncompressed key has 64 bytes")
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl From<PrivateKey> for PublicKey {
    fn from(private_key: PrivateKey) -> Self {
        private_key.public_key()
    }
}

impl From<&PrivateKey> for PublicKey {
    fn from(private_key: &PrivateKey) -> Self {
        private_key.public_key()
    }
}

impl From<PublicKey> for SpPublic {
    fn from(key: PublicKey) -> Self {
        key.0
    }
}

impl From<SpPublic> for PublicKey {
    fn from(public: SpPublic) -> Self {
        Self(public)
    }
}

impl FromStr for PublicKey {
    type Err = SignerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes =
            decode_hex_to_array::<33>(s).map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Self::from_bytes(bytes)
    }
}

#[cfg(feature = "serde")]
impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&format!("0x{}", self.to_hex()))
        } else {
            serializer.serialize_bytes(self.0.as_ref())
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            PublicKey::from_str(&s).map_err(|err| serde::de::Error::custom(err.to_string()))
        } else {
            let bytes = <Vec<u8>>::deserialize(deserializer)?;
            if bytes.len() != 33 {
                return Err(serde::de::Error::custom("Invalid public key length"));
            }
            let mut array = [0u8; 33];
            array.copy_from_slice(&bytes);
            PublicKey::from_bytes(array).map_err(|err| serde::de::Error::custom(err.to_string()))
        }
    }
}
