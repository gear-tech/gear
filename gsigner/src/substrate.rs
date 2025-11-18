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

//! Substrate integration helpers.
//!
//! This module abstractions over `sp_core::Pair` provide a convenient way to
//! reuse Substrate-compatible key types while still benefiting from the
//! higher-level helpers exposed by the scheme-specific modules under
//! [`crate::schemes`].

#[cfg(feature = "keyring")]
use crate::keyring::KeystoreEntry;
use crate::{Result, substrate_utils::crypto_type_id_to_string};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::marker::PhantomData;
use hex;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sp_core::crypto::{AccountId32, CryptoTypeId, Pair as PairTrait, Ss58AddressFormat};

const DEFAULT_SS58_PREFIX: u16 = 137;

/// Trait describing how to bridge between `sp_core::Pair` and the scheme-specific
/// key, signature, and helper types exposed by `gsigner`.
pub trait SubstratePairSpec: Sized {
    /// Concrete Substrate pair type.
    type Pair: PairTrait + Clone;
    /// Private key wrapper type used by `gsigner`.
    type PrivateKey: Clone;
    /// Public key wrapper type used by `gsigner`.
    type PublicKey: Clone;
    /// Signature type produced by the scheme.
    type Signature: Clone;

    /// Generate a new random private key.
    #[cfg(feature = "std")]
    fn generate_private_key() -> Self::PrivateKey;

    /// Construct a private key from a Substrate SURI.
    fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey>;

    /// Construct a private key from an existing Substrate pair.
    fn private_key_from_pair(pair: &Self::Pair) -> Result<Self::PrivateKey>;

    /// Convert a stored private key into an `sp_core::Pair`.
    fn to_sp_pair(private_key: &Self::PrivateKey) -> Result<Self::Pair>;

    /// Sign raw bytes with the provided private key.
    fn sign(private_key: &Self::PrivateKey, message: &[u8]) -> Result<Self::Signature>;

    /// Derive the associated public key from the private key.
    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Export the private key material as bytes.
    fn to_raw_vec(private_key: &Self::PrivateKey) -> Vec<u8>;

    /// Return the raw public key bytes.
    fn public_bytes(public_key: &Self::PublicKey) -> Vec<u8>;

    /// Convert the public key into an AccountId32.
    fn account_id(public_key: &Self::PublicKey) -> AccountId32;

    /// Render the public key as an SS58 string for a specific format.
    fn ss58_with_format(public_key: &Self::PublicKey, format: Ss58AddressFormat) -> Result<String>;

    /// Return the underlying crypto identifier.
    fn key_type_id() -> CryptoTypeId;

    /// Human readable scheme label.
    fn scheme_label() -> &'static str;

    /// Render the public key using the default Vara prefix.
    fn default_ss58(public_key: &Self::PublicKey) -> Result<String> {
        Self::ss58_with_format(public_key, Ss58AddressFormat::custom(DEFAULT_SS58_PREFIX))
    }
}

/// Generic Substrate pair wrapper used across signing schemes.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "S::PrivateKey: Serialize, S::PublicKey: Serialize",
        deserialize = "S::PrivateKey: Deserialize<'de>, S::PublicKey: Deserialize<'de>"
    ))
)]
pub struct GenericSubstratePair<S: SubstratePairSpec> {
    name: String,
    /// Hex-encoded representation of the public key.
    pub address: String,
    private_key: S::PrivateKey,
    public_key: S::PublicKey,
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "serde", serde(default))]
    marker: PhantomData<S>,
}

impl<S> Clone for GenericSubstratePair<S>
where
    S: SubstratePairSpec,
    S::PrivateKey: Clone,
    S::PublicKey: Clone,
{
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            address: self.address.clone(),
            private_key: self.private_key.clone(),
            public_key: self.public_key.clone(),
            marker: PhantomData,
        }
    }
}

impl<S> GenericSubstratePair<S>
where
    S: SubstratePairSpec,
{
    /// Construct a pair wrapper from an existing private key.
    pub fn from_private_key(name: &str, private_key: S::PrivateKey) -> Self {
        let public_key = S::public_key(&private_key);
        let address = format!("0x{}", hex::encode(S::public_bytes(&public_key)));
        Self {
            name: name.to_string(),
            address,
            private_key,
            public_key,
            marker: PhantomData,
        }
    }

    /// Create a pair from a Substrate SURI.
    pub fn from_suri(name: &str, suri: &str, password: Option<&str>) -> Result<Self> {
        let private_key = S::private_key_from_suri(suri, password)?;
        Ok(Self::from_private_key(name, private_key))
    }

    /// Generate a new random pair.
    #[cfg(feature = "std")]
    pub fn generate(name: &str) -> Self {
        Self::from_private_key(name, S::generate_private_key())
    }

    /// Sign a message using the stored private key.
    ///
    /// # Panics
    ///
    /// Panics if signing fails which should not occur for valid stored keys.
    pub fn sign(&self, message: &[u8]) -> S::Signature {
        S::sign(&self.private_key, message)
            .expect("stored private key should always produce signatures; qed")
    }

    /// Return the underlying Substrate pair.
    pub fn to_sp_pair(&self) -> Result<S::Pair> {
        S::to_sp_pair(&self.private_key)
    }

    /// Construct from an existing Substrate pair.
    pub fn from_sp_pair(name: &str, pair: &S::Pair) -> Result<Self> {
        let private_key = S::private_key_from_pair(pair)?;
        Ok(Self::from_private_key(name, private_key))
    }

    /// Returns the private key bytes in their raw representation.
    pub fn to_raw_vec(&self) -> Vec<u8> {
        S::to_raw_vec(&self.private_key)
    }

    /// Returns the raw public key bytes.
    pub fn public_bytes(&self) -> Vec<u8> {
        S::public_bytes(&self.public_key)
    }

    /// Returns the associated account identifier.
    pub fn account_id(&self) -> AccountId32 {
        S::account_id(&self.public_key)
    }

    /// Returns the SS58 encoded address with default Vara prefix.
    pub fn to_ss58check(&self) -> Result<String> {
        S::default_ss58(&self.public_key)
    }

    /// Returns the SS58 encoded address for a custom format.
    pub fn to_ss58check_with_format(&self, format: Ss58AddressFormat) -> Result<String> {
        S::ss58_with_format(&self.public_key, format)
    }

    /// Returns the [`CryptoTypeId`] of the underlying key.
    pub fn crypto_type_id(&self) -> CryptoTypeId {
        S::key_type_id()
    }

    /// Returns the printable key type identifier (e.g. `sr25519`, `ed25519`, `ecdsa`).
    pub fn crypto_type(&self) -> String {
        S::scheme_label().to_string()
    }

    /// Returns the printable key type identifier (e.g. `sr25`).
    pub fn crypto_type_id_string(&self) -> String {
        crypto_type_id_to_string(self.crypto_type_id())
    }

    /// Returns the friendly name assigned to the pair.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Updates the stored name.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Access the underlying private key.
    pub fn private_key(&self) -> &S::PrivateKey {
        &self.private_key
    }

    /// Access the underlying public key.
    pub fn public_key(&self) -> &S::PublicKey {
        &self.public_key
    }
}

#[cfg(all(feature = "keyring", feature = "serde"))]
impl<S> KeystoreEntry for GenericSubstratePair<S>
where
    S: SubstratePairSpec,
    S::PrivateKey: Serialize + for<'de> Deserialize<'de> + Clone,
    S::PublicKey: Serialize + for<'de> Deserialize<'de> + Clone,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.set_name(name);
    }
}

#[cfg(feature = "sr25519")]
mod sr_impl {
    use super::*;
    use crate::{
        schemes::sr25519::{PrivateKey, PublicKey, Signature, Sr25519},
        traits::SignatureScheme,
    };
    use sp_core::{crypto::Ss58Codec, sr25519};

    pub struct Sr25519Spec;

    impl SubstratePairSpec for Sr25519Spec {
        type Pair = sr25519::Pair;
        type PrivateKey = PrivateKey;
        type PublicKey = PublicKey;
        type Signature = Signature;

        #[cfg(feature = "std")]
        fn generate_private_key() -> Self::PrivateKey {
            PrivateKey::random()
        }

        fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
            PrivateKey::from_suri(suri, password)
        }

        #[allow(clippy::clone_on_copy)]
        fn private_key_from_pair(pair: &Self::Pair) -> Result<Self::PrivateKey> {
            Ok(PrivateKey::from_pair(pair.clone()))
        }

        #[allow(clippy::clone_on_copy)]
        fn to_sp_pair(private_key: &Self::PrivateKey) -> Result<Self::Pair> {
            Ok(private_key.as_pair().clone())
        }

        fn sign(private_key: &Self::PrivateKey, message: &[u8]) -> Result<Self::Signature> {
            Sr25519::sign(private_key, message)
        }

        fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
            private_key.public_key()
        }

        fn to_raw_vec(private_key: &Self::PrivateKey) -> Vec<u8> {
            private_key.to_bytes().to_vec()
        }

        fn public_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
            public_key.to_bytes().to_vec()
        }

        fn account_id(public_key: &Self::PublicKey) -> AccountId32 {
            AccountId32::from(public_key.to_bytes())
        }

        fn ss58_with_format(
            public_key: &Self::PublicKey,
            format: Ss58AddressFormat,
        ) -> Result<String> {
            let account = AccountId32::from(public_key.to_bytes());
            Ok(account.to_ss58check_with_version(format))
        }

        fn key_type_id() -> CryptoTypeId {
            sr25519::CRYPTO_ID
        }

        fn scheme_label() -> &'static str {
            "sr25519"
        }
    }

    /// Backwards-compatible alias preserving the previous API surface.
    pub type Sr25519Pair = GenericSubstratePair<Sr25519Spec>;

    /// Historical name retained for compatibility with earlier releases.
    pub type SubstratePair = Sr25519Pair;
}

#[cfg(feature = "sr25519")]
pub use sr_impl::{Sr25519Pair, SubstratePair};

#[cfg(feature = "ed25519")]
mod ed_impl {
    use super::*;
    use crate::{
        schemes::ed25519::{Ed25519, PrivateKey, PublicKey, Signature},
        traits::SignatureScheme,
    };
    use sp_core::{crypto::Ss58Codec, ed25519};

    pub struct Ed25519Spec;

    impl SubstratePairSpec for Ed25519Spec {
        type Pair = ed25519::Pair;
        type PrivateKey = PrivateKey;
        type PublicKey = PublicKey;
        type Signature = Signature;

        #[cfg(feature = "std")]
        fn generate_private_key() -> Self::PrivateKey {
            PrivateKey::random()
        }

        fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
            PrivateKey::from_suri(suri, password)
        }

        #[allow(clippy::clone_on_copy)]
        fn private_key_from_pair(pair: &Self::Pair) -> Result<Self::PrivateKey> {
            Ok(PrivateKey::from_pair(pair.clone()))
        }

        #[allow(clippy::clone_on_copy)]
        fn to_sp_pair(private_key: &Self::PrivateKey) -> Result<Self::Pair> {
            Ok(private_key.as_pair().clone())
        }

        fn sign(private_key: &Self::PrivateKey, message: &[u8]) -> Result<Self::Signature> {
            Ed25519::sign(private_key, message)
        }

        fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
            private_key.public_key()
        }

        fn to_raw_vec(private_key: &Self::PrivateKey) -> Vec<u8> {
            private_key.to_bytes().to_vec()
        }

        fn public_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
            public_key.to_bytes().to_vec()
        }

        fn account_id(public_key: &Self::PublicKey) -> AccountId32 {
            AccountId32::from(public_key.to_bytes())
        }

        fn ss58_with_format(
            public_key: &Self::PublicKey,
            format: Ss58AddressFormat,
        ) -> Result<String> {
            let account = AccountId32::from(public_key.to_bytes());
            Ok(account.to_ss58check_with_version(format))
        }

        fn key_type_id() -> CryptoTypeId {
            ed25519::CRYPTO_ID
        }

        fn scheme_label() -> &'static str {
            "ed25519"
        }
    }

    pub type Ed25519Pair = GenericSubstratePair<Ed25519Spec>;
}

#[cfg(feature = "ed25519")]
pub use ed_impl::Ed25519Pair;

#[cfg(feature = "secp256k1")]
mod ecdsa_impl {
    use super::*;
    use crate::schemes::secp256k1::{PrivateKey, PublicKey, Signature};
    use sp_core::{crypto::Ss58Codec, ecdsa, hashing::blake2_256};

    pub struct Secp256k1Spec;

    impl SubstratePairSpec for Secp256k1Spec {
        type Pair = ecdsa::Pair;
        type PrivateKey = PrivateKey;
        type PublicKey = PublicKey;
        type Signature = Signature;

        #[cfg(feature = "std")]
        fn generate_private_key() -> Self::PrivateKey {
            PrivateKey::random()
        }

        fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
            PrivateKey::from_suri(suri, password)
        }

        fn private_key_from_pair(pair: &Self::Pair) -> Result<Self::PrivateKey> {
            Ok(PrivateKey::from_pair(pair.clone()))
        }

        fn to_sp_pair(private_key: &Self::PrivateKey) -> Result<Self::Pair> {
            Ok(private_key.as_pair().clone())
        }

        fn sign(private_key: &Self::PrivateKey, message: &[u8]) -> Result<Self::Signature> {
            Signature::create(private_key, message)
        }

        fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
            private_key.public_key()
        }

        fn to_raw_vec(private_key: &Self::PrivateKey) -> Vec<u8> {
            private_key.to_bytes().to_vec()
        }

        fn public_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
            public_key.to_bytes().to_vec()
        }

        fn account_id(public_key: &Self::PublicKey) -> AccountId32 {
            let bytes = public_key.to_bytes();
            let hash = blake2_256(&bytes);
            AccountId32::from(hash)
        }

        fn ss58_with_format(
            public_key: &Self::PublicKey,
            format: Ss58AddressFormat,
        ) -> Result<String> {
            Ok(Self::account_id(public_key).to_ss58check_with_version(format))
        }

        fn key_type_id() -> CryptoTypeId {
            ecdsa::CRYPTO_ID
        }

        fn scheme_label() -> &'static str {
            "ecdsa"
        }
    }

    pub type Secp256k1Pair = GenericSubstratePair<Secp256k1Spec>;
}

#[cfg(feature = "secp256k1")]
pub use ecdsa_impl::Secp256k1Pair;

/// Helpers for bridging between the generic pair wrapper and sp_core primitives.
pub mod sp_compat {
    use super::*;

    #[cfg(feature = "sr25519")]
    impl From<sr_impl::Sr25519Pair> for sp_core::sr25519::Pair {
        fn from(pair: sr_impl::Sr25519Pair) -> Self {
            pair.to_sp_pair()
                .expect("sr25519 pair conversion never fails")
        }
    }

    #[cfg(feature = "sr25519")]
    impl From<sp_core::sr25519::Pair> for sr_impl::Sr25519Pair {
        fn from(pair: sp_core::sr25519::Pair) -> Self {
            sr_impl::Sr25519Pair::from_sp_pair("default", &pair)
                .expect("sr25519 pair conversion never fails")
        }
    }

    #[cfg(feature = "ed25519")]
    impl From<ed_impl::Ed25519Pair> for sp_core::ed25519::Pair {
        fn from(pair: ed_impl::Ed25519Pair) -> Self {
            pair.to_sp_pair()
                .expect("ed25519 pair conversion never fails")
        }
    }

    #[cfg(feature = "ed25519")]
    impl From<sp_core::ed25519::Pair> for ed_impl::Ed25519Pair {
        fn from(pair: sp_core::ed25519::Pair) -> Self {
            ed_impl::Ed25519Pair::from_sp_pair("default", &pair)
                .expect("ed25519 pair conversion never fails")
        }
    }

    #[cfg(feature = "secp256k1")]
    impl From<ecdsa_impl::Secp256k1Pair> for sp_core::ecdsa::Pair {
        fn from(pair: ecdsa_impl::Secp256k1Pair) -> Self {
            pair.to_sp_pair()
                .expect("ecdsa pair conversion never fails")
        }
    }

    #[cfg(feature = "secp256k1")]
    impl From<sp_core::ecdsa::Pair> for ecdsa_impl::Secp256k1Pair {
        fn from(pair: sp_core::ecdsa::Pair) -> Self {
            ecdsa_impl::Secp256k1Pair::from_sp_pair("default", &pair)
                .expect("ecdsa pair conversion never fails")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sr25519")]
    #[test]
    fn sr25519_pair_roundtrip() {
        let pair = sr_impl::Sr25519Pair::generate("test");
        assert_eq!(pair.crypto_type_id(), sp_core::sr25519::CRYPTO_ID);
        assert_eq!(pair.public_bytes().len(), 32);
        assert!(!pair.to_raw_vec().is_empty());
        let account: [u8; 32] = pair.account_id().into();
        assert_eq!(account.len(), 32);
        let ss58 = pair.to_ss58check().unwrap();
        assert!(!ss58.is_empty());
        let recovered =
            sr_impl::Sr25519Pair::from_sp_pair("copy", &pair.to_sp_pair().unwrap()).unwrap();
        assert_eq!(pair.public_bytes(), recovered.public_bytes());
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn ed25519_pair_account_id() {
        let pair = ed_impl::Ed25519Pair::generate("test-ed");
        assert_eq!(pair.crypto_type(), "ed25519");
        assert_eq!(pair.public_bytes().len(), 32);
        let account_id: [u8; 32] = pair.account_id().into();
        assert_eq!(account_id.len(), 32);
        let ss58 = pair
            .to_ss58check_with_format(Ss58AddressFormat::custom(DEFAULT_SS58_PREFIX))
            .unwrap();
        assert!(!ss58.is_empty());
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn ecdsa_pair_produces_account_id() {
        let pair = ecdsa_impl::Secp256k1Pair::generate("test-ecdsa");
        assert_eq!(pair.crypto_type_id(), sp_core::ecdsa::CRYPTO_ID);
        assert_eq!(pair.public_bytes().len(), 33);
        let account_id: [u8; 32] = pair.account_id().into();
        assert_eq!(account_id.len(), 32);
        let ss58 = pair
            .to_ss58check_with_format(Ss58AddressFormat::custom(DEFAULT_SS58_PREFIX))
            .unwrap();
        assert!(!ss58.is_empty());
    }
}
