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

//! secp256k1 key types backed by `sp_core` primitives.

use super::Address;
use crate::{error::SignerError, ext::PairExt, utils::decode_hex_to_array};
use alloc::string::{String, ToString};
#[cfg(feature = "serde")]
use alloc::{format, vec::Vec};
use core::{fmt, str::FromStr};
use derive_more::{From, Into};
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
#[derive(Clone, From, Into)]
pub struct PrivateKey(SpPair);

impl PrivateKey {
    #[cfg(feature = "std")]
    pub fn random() -> Self {
        Self(SpPair::generate().0)
    }

    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self, SignerError> {
        SpPair::from_suri_ext(suri, password).map(Self)
    }

    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self, SignerError> {
        SpPair::from_phrase_ext(phrase, password).map(Self)
    }

    pub fn from_pair_seed(seed: Seed) -> Self {
        Self(SpPair::from_seed(&seed))
    }

    pub fn from_seed(seed: [u8; 32]) -> Result<Self, SignerError> {
        SpPair::from_seed_bytes(&seed).map(Self)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.seed()
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.public())
    }

    pub fn as_pair(&self) -> &SpPair {
        &self.0
    }

    pub fn seed(&self) -> Seed {
        self.0.seed()
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrivateKey(0x{}...)", &hex::encode(&self.seed()[..4]))
    }
}

impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
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
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(self.seed().as_ref())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(d)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid private key length"));
        }
        let mut seed = Seed::default();
        seed.as_mut().copy_from_slice(&bytes);
        Ok(PrivateKey::from_pair_seed(seed))
    }
}

/// secp256k1 public key backed by `sp_core::ecdsa::Public` (compressed form).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, From, Into)]
pub struct PublicKey(SpPublic);

impl PublicKey {
    pub fn from_private(private_key: &PrivateKey) -> Self {
        private_key.public_key()
    }

    pub fn from_bytes(bytes: [u8; 33]) -> Result<Self, SignerError> {
        SpPublic::try_from(&bytes[..])
            .map(Self)
            .map_err(|_| SignerError::InvalidKey("Invalid compressed public key".into()))
    }

    pub fn to_bytes(self) -> [u8; 33] {
        self.0
            .as_slice()
            .try_into()
            .expect("compressed key is 33 bytes")
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0.as_slice())
    }

    pub fn to_address(self) -> Address {
        Address::from(self)
    }

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
        write!(f, "PublicKey(0x{})", self.to_hex())
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
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&format!("0x{}", self.to_hex()))
        } else {
            s.serialize_bytes(self.0.as_ref())
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            let s = String::deserialize(d)?;
            PublicKey::from_str(&s).map_err(|e| serde::de::Error::custom(e.to_string()))
        } else {
            let bytes = <Vec<u8>>::deserialize(d)?;
            if bytes.len() != 33 {
                return Err(serde::de::Error::custom("Invalid public key length"));
            }
            let mut arr = [0u8; 33];
            arr.copy_from_slice(&bytes);
            PublicKey::from_bytes(arr).map_err(|e| serde::de::Error::custom(e.to_string()))
        }
    }
}
