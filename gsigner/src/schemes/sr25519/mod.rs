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
    address::{SubstrateAddress, SubstrateCryptoScheme},
    error::{Result, SignerError},
    substrate::{PairSeed, SpPairWrapper},
    traits::SignatureScheme,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use schnorrkel::{
    KEYPAIR_LENGTH, Keypair, PublicKey as SchnorrkelPublicKey, Signature as SchnorrkelSignature,
    signing_context,
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::{
    crypto::{Pair as PairTrait, Ss58AddressFormat, Ss58Codec},
    sr25519::{self, Pair as SpPair, Public as SpPublic, Signature as SpSignature},
};

#[cfg(feature = "std")]
mod signer_ext;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;
#[cfg(all(feature = "serde", feature = "std"))]
pub mod keystore;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::Keyring;
#[cfg(all(feature = "serde", feature = "std"))]
pub use keystore::Keystore;
#[cfg(feature = "std")]
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

/// sr25519 private key backed by `sp_core::sr25519::Pair`.
#[derive(Clone)]
pub struct PrivateKey(SpPairWrapper<SpPair>);

/// Seed type alias matching `sp_core::sr25519::Pair`.
pub type Seed = <SpPair as PairTrait>::Seed;

/// sr25519 public key (32 bytes).
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey(SpPublic);

/// sr25519 signature (64 bytes).
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

    /// Create a private key from a Schnorrkel keypair.
    pub fn from_keypair(keypair: Keypair) -> Self {
        Self(SpPairWrapper::new(keypair.into()))
    }

    /// Create from the underlying seed type.
    pub fn from_pair_seed(seed: Seed) -> Self {
        Self(SpPairWrapper::from_pair_seed(seed))
    }

    /// Create from Substrate SURI (mnemonic, raw seed, dev accounts, derivation paths etc.).
    pub fn from_suri(suri: &str, password: Option<&str>) -> Result<Self> {
        SpPairWrapper::from_suri(suri, password).map(Self)
    }

    /// Create from mnemonic phrase, optionally protected with a password.
    pub fn from_phrase(phrase: &str, password: Option<&str>) -> Result<Self> {
        SpPairWrapper::from_phrase(phrase, password).map(Self)
    }

    /// Create from raw 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        SpPairWrapper::from_seed_bytes(&seed).map(Self)
    }

    /// Export as schnorrkel keypair bytes.
    pub fn to_bytes(&self) -> [u8; KEYPAIR_LENGTH] {
        let raw = self.0.to_raw_vec();
        let mut bytes = [0u8; KEYPAIR_LENGTH];
        let copy_len = core::cmp::min(bytes.len(), raw.len());
        bytes[..copy_len].copy_from_slice(&raw[..copy_len]);
        bytes
    }

    /// Convert to Schnorrkel keypair (for custom signing contexts).
    pub fn keypair(&self) -> Keypair {
        self.0.pair().clone().into()
    }

    /// Access underlying sp_core pair.
    pub fn as_pair(&self) -> &SpPair {
        self.0.pair()
    }

    /// Return public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.pair().public())
    }

    /// Return the raw seed bytes.
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

    fn from_seed(seed: Self::Seed) -> crate::error::Result<Self> {
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
        serializer.serialize_bytes(&self.to_bytes())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != KEYPAIR_LENGTH {
            return Err(serde::de::Error::custom("Invalid sr25519 keypair length"));
        }
        let keypair = Keypair::from_half_ed25519_bytes(&bytes)
            .map_err(|e| serde::de::Error::custom(format!("Invalid keypair bytes: {e:?}")))?;
        Ok(Self::from_keypair(keypair))
    }
}

impl core::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PrivateKey(<redacted>)")
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
        self.0.into()
    }

    /// Return hex-encoded representation.
    pub fn to_hex(self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Return SS58 address using the default Vara prefix.
    pub fn to_address(self) -> Result<SubstrateAddress> {
        SubstrateAddress::new(self.to_bytes(), SubstrateCryptoScheme::Sr25519)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
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
        SchnorrkelPublicKey::from_bytes(key.0.as_ref())
            .map_err(|e| SignerError::InvalidKey(format!("Invalid public key: {e:?}")))
    }
}

impl From<sr25519::Public> for PublicKey {
    fn from(public: sr25519::Public) -> Self {
        Self(public)
    }
}

impl From<PublicKey> for sr25519::Public {
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

    /// Return hex-encoded representation.
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
        serializer.serialize_bytes(self.0.as_ref())
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
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Self(SpSignature::from_raw(arr)))
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
        SchnorrkelSignature::from_bytes(sig.0.as_ref())
            .map_err(|e| SignerError::InvalidSignature(format!("Invalid signature: {e:?}")))
    }
}

impl From<sr25519::Signature> for Signature {
    fn from(sig: sr25519::Signature) -> Self {
        Self(sig)
    }
}

impl From<Signature> for sr25519::Signature {
    fn from(sig: Signature) -> Self {
        sig.0
    }
}

impl SignatureScheme for Sr25519 {
    const NAME: &'static str = "sr25519";

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
        let keypair = private_key.keypair();
        let context = signing_context(SIGNING_CONTEXT);
        Ok(Signature::from(keypair.sign(context.bytes(data))))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        let context = signing_context(SIGNING_CONTEXT);
        let schnorrkel_pub = SchnorrkelPublicKey::try_from(*public_key)?;
        let schnorrkel_sig = SchnorrkelSignature::try_from(*signature)?;

        schnorrkel_pub
            .verify(context.bytes(data), &schnorrkel_sig)
            .map_err(|e| SignerError::Crypto(format!("Verification failed: {e:?}")))
    }

    fn address(public_key: &Self::PublicKey) -> Self::Address {
        public_key
            .to_address()
            .expect("public key bytes always produce valid address")
    }
}

/// Substrate SS58 address helper.
pub fn ss58_address(public_key: &[u8; 32]) -> Result<SubstrateAddress> {
    let format = default_ss58_format();
    let account = sr25519::Public::from_raw(*public_key);
    let ss58 = account.to_ss58check_with_version(format);
    SubstrateAddress::from_ss58(&ss58).map_err(|e| SignerError::InvalidAddress(e.to_string()))
}

#[cfg(all(test, feature = "std"))]
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

    #[test]
    fn suri_and_phrase_compatibility() {
        let bob = PrivateKey::from_suri("//Bob", None).unwrap();
        let bob_pub = Sr25519::public_key(&bob);
        let alice = PrivateKey::from_suri("//Alice", None).unwrap();
        let alice_pub = Sr25519::public_key(&alice);
        assert_ne!(bob_pub.to_bytes(), alice_pub.to_bytes());

        let stash = PrivateKey::from_suri("//Alice//stash", None).unwrap();
        let stash_pub = Sr25519::public_key(&stash);
        assert_ne!(stash_pub.to_bytes(), alice_pub.to_bytes());

        let seed = [1u8; 32];
        let seed_key = PrivateKey::from_seed(seed).unwrap();
        let seed_pub = Sr25519::public_key(&seed_key);
        let seed_again = PrivateKey::from_seed(seed).unwrap();
        assert_eq!(
            seed_pub.to_bytes(),
            Sr25519::public_key(&seed_again).to_bytes()
        );

        let phrase = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
        let phrase_key = PrivateKey::from_phrase(phrase, None).unwrap();
        let phrase_pub = Sr25519::public_key(&phrase_key);
        let phrase_again = PrivateKey::from_phrase(phrase, None).unwrap();
        assert_eq!(
            phrase_pub.to_bytes(),
            Sr25519::public_key(&phrase_again).to_bytes()
        );
    }
}
