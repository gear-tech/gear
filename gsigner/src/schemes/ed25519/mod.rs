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
    substrate::{PairSeed, SpPairWrapper},
    traits::SignatureScheme,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::convert::TryInto;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{
    crypto::{ByteArray, Pair as PairTrait},
    ed25519::{self, Pair as SpPair, Public as SpPublic, Signature as SpSignature},
};

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;
#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::{Keyring, Keystore as KeyringKeystore};

/// Seed type alias matching `sp_core::ed25519::Pair`.
pub type Seed = <SpPair as PairTrait>::Seed;

/// ed25519 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Ed25519;

/// ed25519 private key stored as `sp_core::ed25519::Pair`.
#[derive(Clone)]
pub struct PrivateKey(SpPairWrapper<SpPair>);

/// ed25519 public key (32 bytes).
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey(SpPublic);

/// ed25519 signature (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct Signature(SpSignature);

impl PrivateKey {
    /// Generate a new random private key.
    #[cfg(feature = "std")]
    pub fn random() -> Self {
        Self(SpPairWrapper::generate())
    }

    /// Create a private key from the underlying seed type.
    pub fn from_pair_seed(seed: Seed) -> Self {
        Self(SpPairWrapper::from_pair_seed(seed))
    }

    /// Create a private key from raw 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        SpPairWrapper::from_seed_bytes(&seed).map(Self)
    }

    /// Import from Substrate SURI (mnemonic, dev URIs, derivation paths).
    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self> {
        SpPairWrapper::from_suri(suri, password).map(Self)
    }

    /// Create from mnemonic phrase, optionally protected with a password.
    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self> {
        SpPairWrapper::from_phrase(phrase, password).map(Self)
    }

    /// Export as seed bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.seed()
            .as_ref()
            .try_into()
            .expect("ed25519 seed has fixed length")
    }

    /// Return the underlying pair reference.
    pub fn as_pair(&self) -> &SpPair {
        self.0.pair()
    }

    /// Return corresponding public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.pair().public())
    }

    /// Return the underlying seed.
    pub fn seed(&self) -> Seed {
        PairSeed::pair_seed(self.as_pair())
    }

    /// Construct from an existing Substrate pair.
    pub(crate) fn from_pair(pair: SpPair) -> Self {
        Self(SpPairWrapper::new(pair))
    }
}

impl crate::traits::SeedableKey for PrivateKey {
    type Seed = Seed;

    fn from_seed(seed: Self::Seed) -> Result<Self> {
        Ok(PrivateKey::from_pair_seed(seed))
    }

    fn seed(&self) -> Self::Seed {
        PrivateKey::seed(self)
    }
}

#[cfg(feature = "serde")]
impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.seed().as_ref())
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
        let mut seed = Seed::default();
        seed.as_mut().copy_from_slice(&bytes);
        Ok(PrivateKey::from_pair_seed(seed))
    }
}

impl core::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
    }
}

impl core::fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{}", hex::encode(self.seed().as_ref()))
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for PrivateKey {}

impl PublicKey {
    /// Construct from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SpPublic::from_raw(bytes))
    }

    /// Return raw public key bytes.
    pub fn to_bytes(self) -> [u8; 32] {
        self.0
            .as_slice()
            .try_into()
            .expect("ed25519 public key has fixed length")
    }

    /// Return hex-encoded representation.
    pub fn to_hex(self) -> String {
        hex::encode(self.0.as_slice())
    }

    /// Return SS58 address using the default Vara prefix.
    pub fn to_address(self) -> Result<SubstrateAddress> {
        SubstrateAddress::new(self.to_bytes(), SubstrateCryptoScheme::Ed25519)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }
}

impl core::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PublicKey(0x{})", self.to_hex())
    }
}

impl core::fmt::Display for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl From<ed25519::Public> for PublicKey {
    fn from(public: ed25519::Public) -> Self {
        Self(public)
    }
}

impl From<PublicKey> for ed25519::Public {
    fn from(key: PublicKey) -> Self {
        key.0
    }
}

impl Signature {
    /// Construct signature from raw bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(SpSignature::from_raw(bytes))
    }

    /// Return raw signature bytes.
    pub fn to_bytes(self) -> [u8; 64] {
        self.0.into()
    }

    /// Return hex representation of the signature.
    pub fn to_hex(self) -> String {
        hex::encode(self.to_bytes())
    }
}

impl core::fmt::Debug for Signature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Signature(0x{})", self.to_hex())
    }
}

#[cfg(feature = "serde")]
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.to_bytes())
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
        Ok(Self(SpSignature::from_raw(arr)))
    }
}

impl SignatureScheme for Ed25519 {
    const NAME: &'static str = "ed25519";

    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Signature = Signature;
    type Address = SubstrateAddress;
    type Digest = Vec<u8>;

    #[cfg(feature = "std")]
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey) {
        let private_key = PrivateKey::random();
        let public_key = private_key.public_key();
        (private_key, public_key)
    }

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        private_key.public_key()
    }

    fn public_key_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
        public_key.to_bytes().to_vec()
    }

    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature> {
        Ok(Signature(private_key.as_pair().sign(data)))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        if SpPair::verify(&signature.0, data, &public_key.0) {
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
}

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
impl crate::keyring::KeyringScheme for Ed25519 {
    type Keystore = keyring::Keystore;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_ED
    }

    fn keystore_from_private(
        name: &str,
        private_key: &Self::PrivateKey,
        password: Option<&str>,
    ) -> Result<Self::Keystore> {
        Ok(Self::Keystore::from_private_key_with_password(
            name,
            private_key.clone(),
            password,
        )?)
    }

    fn keystore_private(
        keystore: &Self::Keystore,
        password: Option<&str>,
    ) -> Result<Self::PrivateKey> {
        Ok(keystore.private_key_with_password(password)?)
    }

    fn keystore_public(keystore: &Self::Keystore) -> Result<Self::PublicKey> {
        Ok(keystore.public_key()?)
    }

    fn keystore_address(keystore: &Self::Keystore) -> Result<Self::Address> {
        Ok(keystore.address()?)
    }
}

#[cfg(all(test, feature = "std"))]
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
