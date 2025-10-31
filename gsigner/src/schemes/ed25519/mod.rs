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

//! ed25519 signature scheme (Substrate-compatible).

use crate::{
    address::{SubstrateAddress, SubstrateCryptoScheme},
    error::{Result, SignerError},
    traits::SignatureScheme,
};
use alloc::vec::Vec;
use rand::{RngCore, rngs::OsRng};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{
    Pair as _,
    ed25519::{self, Pair as SpPair},
};
use zeroize::Zeroizing;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;
#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::{Keyring, Keystore as KeyringKeystore};

/// ed25519 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Ed25519;

/// ed25519 private key stored as zeroizing seed bytes.
#[derive(Clone)]
pub struct PrivateKey {
    seed: Zeroizing<[u8; 32]>,
}

/// ed25519 public key (32 bytes).
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct PublicKey([u8; 32]);

/// ed25519 signature (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct Signature([u8; 64]);

impl PrivateKey {
    /// Generate a new random private key.
    pub fn random() -> Self {
        let mut seed = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(seed.as_mut());
        Self { seed }
    }

    /// Create a private key from raw 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        SpPair::from_seed_slice(&seed).map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Ok(Self {
            seed: Zeroizing::new(seed),
        })
    }
    /// Import from Substrate SURI (mnemonic, dev URIs, derivation paths).
    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self> {
        let (pair, _) = SpPair::from_string_with_seed(suri, password)
            .map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Ok(Self::from_sp_pair(pair))
    }

    /// Create from mnemonic phrase, optionally protected with a password.
    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self> {
        let (pair, _) = SpPair::from_phrase(phrase, password)
            .map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Ok(Self::from_sp_pair(pair))
    }

    /// Export as seed bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        *self.seed
    }

    fn pair(&self) -> Result<SpPair> {
        SpPair::from_seed_slice(self.seed.as_ref())
            .map_err(|e| SignerError::InvalidKey(e.to_string()))
    }

    fn from_sp_pair(pair: SpPair) -> Self {
        let raw = pair.to_raw_vec();
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.as_mut().copy_from_slice(&raw[..32]);
        Self { seed }
    }
}

#[cfg(feature = "serde")]
impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.seed.as_ref())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid ed25519 seed length"));
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Self::from_seed(array).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

impl core::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.seed.as_ref() == other.seed.as_ref()
    }
}

impl Eq for PrivateKey {}

impl Default for PrivateKey {
    fn default() -> Self {
        Self {
            seed: Zeroizing::new([0u8; 32]),
        }
    }
}

impl PublicKey {
    /// Construct from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return raw public key bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Return hex-encoded representation.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Return SS58 address using the default Vara prefix.
    pub fn to_address(&self) -> Result<SubstrateAddress> {
        SubstrateAddress::new(self.0, SubstrateCryptoScheme::Ed25519)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }
}

impl core::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PublicKey(0x{})", hex::encode(self.0))
    }
}

impl From<ed25519::Public> for PublicKey {
    fn from(public: ed25519::Public) -> Self {
        Self(public.0)
    }
}

impl From<PublicKey> for ed25519::Public {
    fn from(key: PublicKey) -> Self {
        ed25519::Public::from_raw(key.0)
    }
}

impl Signature {
    /// Construct signature from raw bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Return raw signature bytes.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0
    }

    /// Return hex representation of the signature.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl core::fmt::Debug for Signature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Signature(0x{})", hex::encode(self.0))
    }
}

#[cfg(feature = "serde")]
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid ed25519 signature length"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

impl From<ed25519::Signature> for Signature {
    fn from(sig: ed25519::Signature) -> Self {
        Self(sig.0)
    }
}

impl From<Signature> for ed25519::Signature {
    fn from(sig: Signature) -> Self {
        ed25519::Signature::from_raw(sig.0)
    }
}

impl SignatureScheme for Ed25519 {
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Signature = Signature;
    type Address = SubstrateAddress;
    type Digest = Vec<u8>;

    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey) {
        let private_key = PrivateKey::random();
        let public_key = Self::public_key(&private_key);
        (private_key, public_key)
    }

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        let pair = private_key
            .pair()
            .expect("stored ed25519 seed is always valid");
        PublicKey::from(pair.public())
    }

    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature> {
        let pair = private_key.pair()?;
        Ok(Signature::from(pair.sign(data)))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        let sp_signature: ed25519::Signature = (*signature).into();
        let sp_public: ed25519::Public = (*public_key).into();
        if SpPair::verify(&sp_signature, data, &sp_public) {
            Ok(())
        } else {
            Err(SignerError::Crypto("Verification failed".to_string()))
        }
    }

    fn address(public_key: &Self::PublicKey) -> Self::Address {
        public_key
            .to_address()
            .expect("public key bytes always produce valid address")
    }

    fn scheme_name() -> &'static str {
        "ed25519"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generation() {
        let (private_key, public_key) = Ed25519::generate_keypair();
        let derived = Ed25519::public_key(&private_key);
        assert_eq!(public_key, derived);
    }

    #[test]
    fn sign_and_verify() {
        let (private_key, public_key) = Ed25519::generate_keypair();
        let message = b"hello world";

        let signature = Ed25519::sign(&private_key, message).unwrap();
        Ed25519::verify(&public_key, message, &signature).unwrap();
    }
}
