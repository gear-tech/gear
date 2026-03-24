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

//! Unified cryptographic scheme trait.
//!
//! This module provides the [`CryptoScheme`] trait which unifies all cryptographic
//! operations for a signature scheme including:
//! - Key generation and derivation
//! - Signing and verification
//! - Address derivation
//! - Keystore integration (serialization/deserialization)
//!
//! Previously these were split across `SignatureScheme`, `KeyringScheme`, and `KeyCodec`.

use crate::error::Result;
use alloc::{string::String, vec::Vec};
use core::{fmt::Debug, hash::Hash};

/// Unified trait for cryptographic signature schemes.
///
/// This trait combines all operations needed to work with a signature scheme:
/// - Core cryptographic operations (generate, sign, verify)
/// - Type definitions (keys, signatures, addresses)
/// - Keystore integration for persistent storage
///
/// # Type Parameters
///
/// Implementors must define associated types for:
/// - `PrivateKey` - The private/secret key type
/// - `PublicKey` - The public key type
/// - `Signature` - The signature type
/// - `Address` - The address type derived from public keys
///
/// # Example
///
/// ```rust,ignore
/// use gsigner::{CryptoScheme, secp256k1::Secp256k1};
///
/// // Generate a keypair
/// let (private, public) = Secp256k1::generate_keypair();
///
/// // Sign data
/// let signature = Secp256k1::sign(&private, b"hello")?;
///
/// // Verify
/// Secp256k1::verify(&public, b"hello", &signature)?;
///
/// // Get address
/// let address = Secp256k1::to_address(&public);
/// ```
pub trait CryptoScheme: Debug + Clone + Copy + Send + Sync + 'static {
    /// Human-readable name of the scheme (e.g., "secp256k1", "sr25519", "ed25519").
    const NAME: &'static str;

    /// Directory namespace for keyring storage segregation.
    const NAMESPACE: &'static str;

    /// Size of the public key in bytes.
    const PUBLIC_KEY_SIZE: usize;

    /// Size of the signature in bytes.
    const SIGNATURE_SIZE: usize;

    /// The private key type for this scheme.
    type PrivateKey: Clone + Debug + Send + Sync;

    /// The public key type for this scheme.
    type PublicKey: Clone + Debug + Send + Sync + Eq + Hash + Ord;

    /// The signature type for this scheme.
    type Signature: Clone + Debug + Send + Sync;

    /// The address type for this scheme.
    type Address: Clone + Debug + Send + Sync + PartialEq;

    /// The seed type for deterministic key generation.
    type Seed: Clone + Default + AsRef<[u8]> + AsMut<[u8]> + Send + Sync + 'static;

    /// Generate a new random keypair.
    #[cfg(feature = "std")]
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey);

    /// Derive the public key from a private key.
    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Sign data with a private key.
    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature>;

    /// Verify a signature against a public key and data.
    fn verify(public_key: &Self::PublicKey, data: &[u8], signature: &Self::Signature)
    -> Result<()>;

    /// Derive an address from a public key.
    fn to_address(public_key: &Self::PublicKey) -> Self::Address;

    /// Serialize a public key to bytes.
    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8>;

    /// Deserialize a public key from bytes.
    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey>;

    /// Serialize a public key to hex string.
    fn public_key_to_hex(public_key: &Self::PublicKey) -> String {
        hex::encode(Self::public_key_to_bytes(public_key))
    }

    /// Deserialize a public key from hex string.
    fn public_key_from_hex(hex_str: &str) -> Result<Self::PublicKey> {
        let bytes = crate::utils::decode_hex(hex_str)?;
        Self::public_key_from_bytes(&bytes)
    }

    /// Serialize a signature to bytes.
    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8>;

    /// Deserialize a signature from bytes.
    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature>;

    /// Serialize a signature to hex string.
    fn signature_to_hex(signature: &Self::Signature) -> String {
        hex::encode(Self::signature_to_bytes(signature))
    }

    /// Deserialize a signature from hex string.
    fn signature_from_hex(hex_str: &str) -> Result<Self::Signature> {
        let bytes = crate::utils::decode_hex(hex_str)?;
        Self::signature_from_bytes(&bytes)
    }

    /// Format address for display.
    fn address_to_string(address: &Self::Address) -> String;

    /// Create a private key from a seed.
    fn private_key_from_seed(seed: Self::Seed) -> Result<Self::PrivateKey>;

    /// Extract the seed from a private key.
    fn private_key_to_seed(private_key: &Self::PrivateKey) -> Self::Seed;

    /// Import a private key from SURI (Substrate URI format).
    /// Supports mnemonics, dev accounts (//Alice), derivation paths, and hex seeds.
    #[cfg(feature = "std")]
    fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey>;
}

/// Operations that a keystore entry must support.
#[cfg(feature = "keyring")]
pub trait KeystoreOps<S: CryptoScheme>: crate::keyring::KeystoreEntry {
    /// Create a keystore from a private key with optional encryption.
    fn from_private_key(
        name: &str,
        private_key: &S::PrivateKey,
        password: Option<&str>,
    ) -> Result<Self>
    where
        Self: Sized;

    /// Extract the private key, decrypting if necessary.
    fn to_private_key(&self, password: Option<&str>) -> Result<S::PrivateKey>;

    /// Get the public key.
    fn to_public_key(&self) -> Result<S::PublicKey>;

    /// Get the address.
    fn to_address(&self) -> Result<S::Address>;
}

#[cfg(test)]
mod tests {
    // Compile-time check that the trait is object-safe where possible
    // (Note: CryptoScheme itself isn't fully object-safe due to associated types,
    // but we can verify the basic structure compiles)

    fn _assert_send_sync<T: Send + Sync>() {}

    #[cfg(feature = "secp256k1")]
    fn _check_secp256k1_impl() {
        use crate::schemes::secp256k1::Secp256k1;
        _assert_send_sync::<Secp256k1>();
    }

    #[cfg(feature = "sr25519")]
    fn _check_sr25519_impl() {
        use crate::schemes::sr25519::Sr25519;
        _assert_send_sync::<Sr25519>();
    }

    #[cfg(feature = "ed25519")]
    fn _check_ed25519_impl() {
        use crate::schemes::ed25519::Ed25519;
        _assert_send_sync::<Ed25519>();
    }
}
