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

//! sr25519 Schnorrkel signature scheme (Substrate-compatible).
//!
//! Thin wrappers around sp_core types providing Debug, serde, and additional methods.

use crate::{
    address::{SubstrateAddress, SubstrateCryptoScheme},
    error::{Result, SignerError},
    ext::PairExt,
};
use alloc::{format, string::String, vec::Vec};
use derive_more::{AsRef, From, Into};
use schnorrkel::{
    KEYPAIR_LENGTH, Keypair, PublicKey as SchnorrkelPublicKey, Signature as SchnorrkelSignature,
    signing_context,
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{
    crypto::Pair as PairTrait,
    sr25519::{Pair as SpPair, Public as SpPublic, Signature as SpSignature},
};

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
mod signer_ext;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::{Keyring, Keystore, Sr25519Codec};
#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub use signer_ext::Sr25519SignerExt;

const SIGNING_CONTEXT: &[u8] = b"gsigner";

/// sr25519 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Sr25519;

/// Seed type alias.
pub type Seed = <SpPair as PairTrait>::Seed;

/// sr25519 private key wrapper (sp_core::sr25519::Pair lacks Debug).
#[derive(Clone, From)]
pub struct PrivateKey(SpPair);

/// sr25519 public key (32 bytes).
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, From, Into, AsRef)]
pub struct PublicKey(SpPublic);

/// sr25519 signature (64 bytes).
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
#[derive(Clone, Copy, PartialEq, Eq, From, Into, AsRef)]
pub struct Signature(SpSignature);

impl PrivateKey {
    #[cfg(feature = "std")]
    pub fn random() -> Self {
        Self(SpPair::generate().0)
    }

    pub fn from_keypair(keypair: Keypair) -> Self {
        Self(keypair.into())
    }

    pub fn from_pair_seed(seed: Seed) -> Self {
        Self(SpPair::from_seed(&seed))
    }

    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self> {
        SpPair::from_suri_ext(suri, password).map(Self)
    }

    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self> {
        SpPair::from_phrase_ext(phrase, password).map(Self)
    }

    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        SpPair::from_seed_bytes(&seed).map(Self)
    }

    pub fn to_bytes(&self) -> [u8; KEYPAIR_LENGTH] {
        self.keypair().to_half_ed25519_bytes()
    }

    pub fn keypair(&self) -> Keypair {
        self.0.clone().into()
    }

    pub fn as_pair(&self) -> &SpPair {
        &self.0
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.public())
    }

    pub fn seed(&self) -> Seed {
        self.0.seed_bytes()
    }
}

impl core::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PrivateKey(0x{}...)", &hex::encode(&self.seed()[..4]))
    }
}

#[cfg(feature = "keyring")]
impl crate::keyring::PrivateKeyOps for PrivateKey {
    type PublicKey = PublicKey;

    fn public_key(&self) -> Self::PublicKey {
        self.public_key()
    }

    fn random() -> Self {
        Self::random()
    }
}

#[cfg(feature = "keyring")]
impl crate::keyring::PublicKeyBytes for PublicKey {
    fn to_bytes(&self) -> [u8; 32] {
        // Use the inner sp_core type's Into impl directly to avoid recursion
        self.0.into()
    }

    fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(sp_core::sr25519::Public::from_raw(bytes))
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for PrivateKey {}

#[cfg(feature = "serde")]
impl Serialize for PrivateKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> core::result::Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.to_bytes())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(d)?;
        if bytes.len() != KEYPAIR_LENGTH {
            return Err(serde::de::Error::custom("Invalid sr25519 keypair length"));
        }
        Keypair::from_half_ed25519_bytes(&bytes)
            .map(Self::from_keypair)
            .map_err(|e| serde::de::Error::custom(format!("Invalid keypair: {e:?}")))
    }
}

impl PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SpPublic::from_raw(bytes))
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.into()
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn to_address(self) -> Result<SubstrateAddress> {
        SubstrateAddress::new(self.to_bytes(), SubstrateCryptoScheme::Sr25519)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }

    pub fn to_schnorrkel(self) -> Result<SchnorrkelPublicKey> {
        SchnorrkelPublicKey::from_bytes(self.0.as_ref())
            .map_err(|e| SignerError::InvalidKey(format!("Invalid public key: {e:?}")))
    }
}

impl core::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PublicKey(0x{})", self.to_hex())
    }
}

impl From<SchnorrkelPublicKey> for PublicKey {
    fn from(key: SchnorrkelPublicKey) -> Self {
        Self(SpPublic::from_raw(key.to_bytes()))
    }
}

impl TryFrom<PublicKey> for SchnorrkelPublicKey {
    type Error = SignerError;
    fn try_from(key: PublicKey) -> Result<Self> {
        key.to_schnorrkel()
    }
}

impl Signature {
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(SpSignature::from_raw(bytes))
    }

    pub fn to_bytes(self) -> [u8; 64] {
        self.0.into()
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn to_schnorrkel(self) -> Result<SchnorrkelSignature> {
        SchnorrkelSignature::from_bytes(self.0.as_ref())
            .map_err(|e| SignerError::InvalidSignature(format!("Invalid signature: {e:?}")))
    }
}

impl core::fmt::Debug for Signature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Signature(0x{}...)", &hex::encode(&self.to_bytes()[..8]))
    }
}

impl From<SchnorrkelSignature> for Signature {
    fn from(sig: SchnorrkelSignature) -> Self {
        Self(SpSignature::from_raw(sig.to_bytes()))
    }
}

impl TryFrom<Signature> for SchnorrkelSignature {
    type Error = SignerError;
    fn try_from(sig: Signature) -> Result<Self> {
        sig.to_schnorrkel()
    }
}

#[cfg(feature = "serde")]
impl Serialize for Signature {
    fn serialize<S: serde::Serializer>(&self, s: S) -> core::result::Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.to_bytes())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(d)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self::from_bytes(arr))
    }
}

impl crate::scheme::CryptoScheme for Sr25519 {
    const NAME: &'static str = "sr25519";
    const NAMESPACE: &'static str = "sr";
    const PUBLIC_KEY_SIZE: usize = 32;
    const SIGNATURE_SIZE: usize = 64;

    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Signature = Signature;
    type Address = SubstrateAddress;
    type Seed = Seed;

    #[cfg(feature = "std")]
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey) {
        let private = PrivateKey::random();
        let public = private.public_key();
        (private, public)
    }

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        private_key.public_key()
    }

    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature> {
        let keypair = private_key.keypair();
        let ctx = signing_context(SIGNING_CONTEXT);
        Ok(Signature::from(keypair.sign(ctx.bytes(data))))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        let ctx = signing_context(SIGNING_CONTEXT);
        let pub_key = public_key.to_schnorrkel()?;
        let sig = signature.to_schnorrkel()?;
        pub_key
            .verify(ctx.bytes(data), &sig)
            .map_err(|e| SignerError::Crypto(format!("Verification failed: {e:?}")))
    }

    fn to_address(public_key: &Self::PublicKey) -> Self::Address {
        public_key.to_address().expect("valid address")
    }

    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
        public_key.to_bytes().to_vec()
    }

    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey> {
        if bytes.len() != 32 {
            return Err(SignerError::InvalidKey(format!(
                "Invalid sr25519 public key length: expected 32, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(PublicKey::from_bytes(arr))
    }

    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8> {
        signature.to_bytes().to_vec()
    }

    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature> {
        if bytes.len() != 64 {
            return Err(SignerError::InvalidSignature(format!(
                "Invalid sr25519 signature length: expected 64, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(bytes);
        Ok(Signature::from_bytes(arr))
    }

    fn address_to_string(address: &Self::Address) -> String {
        address.to_string()
    }

    fn private_key_from_seed(seed: Self::Seed) -> Result<Self::PrivateKey> {
        Ok(PrivateKey::from_pair_seed(seed))
    }

    fn private_key_to_seed(private_key: &Self::PrivateKey) -> Self::Seed {
        private_key.seed()
    }

    #[cfg(feature = "std")]
    fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        PrivateKey::from_suri(suri, password)
    }
}

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
impl crate::keyring::KeyringScheme for Sr25519 {
    type Keystore = keyring::Keystore;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_SR
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
    use crate::scheme::CryptoScheme;

    #[test]
    fn keypair_generation() {
        let (private, public) = Sr25519::generate_keypair();
        assert_eq!(public, Sr25519::public_key(&private));
    }

    #[test]
    fn sign_and_verify() {
        let (private, public) = Sr25519::generate_keypair();
        let sig = Sr25519::sign(&private, b"hello").unwrap();
        Sr25519::verify(&public, b"hello", &sig).unwrap();
    }

    #[test]
    fn deterministic_suri() {
        let a1 = PrivateKey::from_suri("//Alice", None).unwrap();
        let a2 = PrivateKey::from_suri("//Alice", None).unwrap();
        assert_eq!(a1.public_key().to_bytes(), a2.public_key().to_bytes());
    }
}
