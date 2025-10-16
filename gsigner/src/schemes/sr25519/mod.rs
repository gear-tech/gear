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

//! sr25519 Schnorrkel signature scheme (Substrate-compatible).

use crate::{
    address::SubstrateAddress,
    error::{Result, SignerError},
    traits::SignatureScheme,
};
use schnorrkel::{
    KEYPAIR_LENGTH, Keypair, PublicKey as SchnorrkelPublicKey, Signature as SchnorrkelSignature,
    signing_context,
};
use serde::{Deserialize, Serialize};
use sp_core::{
    Pair as _,
    crypto::{Ss58AddressFormat, Ss58Codec},
    sr25519::{self, Pair as SpPair},
};
use std::vec::Vec;
use zeroize::Zeroizing;

pub mod keyring;
pub mod keystore;
mod signer_ext;

pub use keyring::Keyring;
pub use keystore::Keystore;
pub use signer_ext::Sr25519SignerExt;

const SIGNING_CONTEXT: &[u8] = b"gsigner";
const DEFAULT_SS58_PREFIX: u16 = 137;

#[inline]
fn default_ss58_format() -> Ss58AddressFormat {
    Ss58AddressFormat::custom(DEFAULT_SS58_PREFIX)
}

/// sr25519 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Sr25519;

/// sr25519 private key stored as zeroizing keypair bytes (schnorrkel half-ed25519 form).
#[derive(Clone)]
pub struct PrivateKey {
    keypair: Zeroizing<[u8; KEYPAIR_LENGTH]>,
}

/// sr25519 public key (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct PublicKey([u8; 32]);

/// sr25519 signature (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
pub struct Signature([u8; 64]);

impl PrivateKey {
    /// Generate a new random private key.
    pub fn random() -> Self {
        Self::from_keypair(Keypair::generate())
    }

    /// Reconstruct schnorrkel keypair from stored bytes.
    pub(crate) fn keypair(&self) -> Result<Keypair> {
        Keypair::from_half_ed25519_bytes(self.keypair.as_ref())
            .map_err(|e| SignerError::Crypto(format!("Invalid keypair bytes: {e:?}")))
    }

    /// Create a private key from schnorrkel keypair.
    pub fn from_keypair(keypair: Keypair) -> Self {
        let mut bytes = Zeroizing::new([0u8; KEYPAIR_LENGTH]);
        bytes
            .as_mut()
            .copy_from_slice(&keypair.to_half_ed25519_bytes());
        Self { keypair: bytes }
    }

    /// Create from Substrate SURI (mnemonic, raw seed, dev accounts, derivation paths etc.).
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

    /// Create from raw 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        let pair =
            SpPair::from_seed_slice(&seed).map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        Ok(Self::from_sp_pair(pair))
    }

    /// Export as schnorrkel keypair bytes.
    pub fn to_bytes(&self) -> [u8; KEYPAIR_LENGTH] {
        *self.keypair
    }

    fn from_sp_pair(pair: SpPair) -> Self {
        let schnorrkel: Keypair = pair.into();
        Self::from_keypair(schnorrkel)
    }
}

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.keypair.as_ref())
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != KEYPAIR_LENGTH {
            return Err(serde::de::Error::custom("Invalid sr25519 keypair length"));
        }
        let mut array = [0u8; KEYPAIR_LENGTH];
        array.copy_from_slice(&bytes);
        Ok(Self {
            keypair: Zeroizing::new(array),
        })
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.keypair.as_ref() == other.keypair.as_ref()
    }
}

impl Eq for PrivateKey {}

impl Default for PrivateKey {
    fn default() -> Self {
        Self {
            keypair: Zeroizing::new([0u8; KEYPAIR_LENGTH]),
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
        SubstrateAddress::new(self.0).map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey(0x{})", hex::encode(self.0))
    }
}

impl From<SchnorrkelPublicKey> for PublicKey {
    fn from(key: SchnorrkelPublicKey) -> Self {
        Self(key.to_bytes())
    }
}

impl TryFrom<PublicKey> for SchnorrkelPublicKey {
    type Error = SignerError;

    fn try_from(key: PublicKey) -> Result<Self> {
        SchnorrkelPublicKey::from_bytes(&key.0)
            .map_err(|e| SignerError::InvalidKey(format!("Invalid public key: {e:?}")))
    }
}

impl From<sr25519::Public> for PublicKey {
    fn from(public: sr25519::Public) -> Self {
        Self(public.0)
    }
}

impl From<PublicKey> for sr25519::Public {
    fn from(key: PublicKey) -> Self {
        sr25519::Public::from_raw(key.0)
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
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Signature(0x{})", hex::encode(self.0))
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

impl From<SchnorrkelSignature> for Signature {
    fn from(sig: SchnorrkelSignature) -> Self {
        Self(sig.to_bytes())
    }
}

impl TryFrom<Signature> for SchnorrkelSignature {
    type Error = SignerError;

    fn try_from(sig: Signature) -> Result<Self> {
        SchnorrkelSignature::from_bytes(&sig.0)
            .map_err(|e| SignerError::InvalidSignature(format!("Invalid signature: {e:?}")))
    }
}

impl From<sr25519::Signature> for Signature {
    fn from(sig: sr25519::Signature) -> Self {
        Self(sig.0)
    }
}

impl From<Signature> for sr25519::Signature {
    fn from(sig: Signature) -> Self {
        sr25519::Signature::from_raw(sig.0)
    }
}

impl SignatureScheme for Sr25519 {
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
        let keypair = private_key
            .keypair()
            .expect("stored sr25519 keypair is always valid");
        PublicKey::from(keypair.public)
    }

    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature> {
        let keypair = private_key.keypair()?;
        let context = signing_context(SIGNING_CONTEXT);
        Ok(Signature::from(keypair.sign(context.bytes(data))))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        let context = signing_context(SIGNING_CONTEXT);
        let schnorrkel_pub = SchnorrkelPublicKey::from_bytes(&public_key.0)
            .map_err(|e| SignerError::InvalidKey(format!("Invalid public key: {e:?}")))?;
        let schnorrkel_sig = SchnorrkelSignature::from_bytes(&signature.0)
            .map_err(|e| SignerError::InvalidSignature(format!("Invalid signature: {e:?}")))?;

        schnorrkel_pub
            .verify(context.bytes(data), &schnorrkel_sig)
            .map_err(|e| SignerError::Crypto(format!("Verification failed: {e:?}")))
    }

    fn address(public_key: &Self::PublicKey) -> Self::Address {
        public_key
            .to_address()
            .expect("public key bytes always produce valid address")
    }

    fn scheme_name() -> &'static str {
        "sr25519"
    }
}

/// Substrate SS58 address helper.
pub fn ss58_address(public_key: &[u8; 32]) -> Result<SubstrateAddress> {
    let format = default_ss58_format();
    let account = sr25519::Public::from_raw(*public_key);
    let ss58 = account.to_ss58check_with_version(format);
    SubstrateAddress::from_ss58(&ss58).map_err(|e| SignerError::InvalidAddress(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generation() {
        let (private_key, public_key) = Sr25519::generate_keypair();
        let derived = Sr25519::public_key(&private_key);
        assert_eq!(public_key, derived);
    }

    #[test]
    fn sign_and_verify() {
        let (private_key, public_key) = Sr25519::generate_keypair();
        let message = b"hello world";

        let signature = Sr25519::sign(&private_key, message).unwrap();
        Sr25519::verify(&public_key, message, &signature).unwrap();
    }

    #[test]
    fn address_derivation() {
        let (_, public_key) = Sr25519::generate_keypair();
        let address = Sr25519::address(&public_key);
        assert_eq!(address.as_bytes().len(), 32);
    }

    #[test]
    fn deterministic_suri() {
        let alice = PrivateKey::from_suri("//Alice", None).unwrap();
        let alice_pub = Sr25519::public_key(&alice);

        let alice2 = PrivateKey::from_suri("//Alice", None).unwrap();
        let alice_pub2 = Sr25519::public_key(&alice2);

        assert_eq!(alice_pub.to_bytes(), alice_pub2.to_bytes());
    }
}
