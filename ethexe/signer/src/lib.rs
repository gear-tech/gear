// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Signer library for ethexe.
//!
//! The crate defines types and related logic for private keys, public keys types,
//! cryptographic signatures and ethereum address.
//!
//! Cryptographic instrumentary of the crate is based on secp256k1 standard
//! using [secp256k1](https://crates.io/crates/secp256k1) crate, but all the
//! machinery used is wrapped in the crate's types.

mod address;
mod digest;
mod private_key;
mod public_key;
mod signature;
mod utils;

// Exports
pub use address::Address;
pub use digest::{Digest, ToDigest};
pub use private_key::PrivateKey;
pub use public_key::PublicKey;
pub use sha3;
pub use signature::Signature;

use anyhow::{bail, Result};
use secp256k1::hashes::hex::{Case, DisplayHex};
use signature::RawSignature;
use std::{fs, path::PathBuf, str::FromStr};

/// Signer which signs data using owned key store.
#[derive(Debug, Clone)]
pub struct Signer {
    key_store: PathBuf,
}

impl Signer {
    /// Create a new signer with a key store location.
    pub fn new(key_store: PathBuf) -> Result<Self> {
        fs::create_dir_all(key_store.as_path())?;

        Ok(Self { key_store })
    }

    /// Create a new signer with a key temporary key store location.
    pub fn tmp() -> Self {
        let temp_dir = tempfile::tempdir().expect("Cannot create temp dir for keys");
        Self {
            key_store: temp_dir.into_path(),
        }
    }

    /// Create a ECDSA recoverable signature with `Electrum` notation for the `v` value.
    ///
    /// For more info about `v` value read [`RawSignature`] docs.
    pub fn raw_sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<RawSignature> {
        let private_key = self.get_private_key(public_key)?;

        RawSignature::create_for_digest(private_key, digest)
    }

    /// Create a ECDSA recoverable signature.
    // TODO #4365
    pub fn sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<Signature> {
        let private_key = self.get_private_key(public_key)?;

        Signature::create_for_digest(private_key, digest)
    }

    /// Create a ECDSA recoverable signature for the raw bytes data.
    pub fn sign(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        self.sign_digest(public_key, data.to_digest())
    }

    /// Create a ECDSA recoverable signature for the raw bytes data with
    /// an ethereum address provided instead of the public key.
    ///
    /// If the private key for the ethereum address is stored, the signature will be returned.
    pub fn sign_with_addr(&self, address: Address, data: &[u8]) -> Result<Signature> {
        match self.get_key_by_addr(address)? {
            Some(public_key) => self.sign(public_key, data),
            None => bail!("Address not found: {}", address),
        }
    }

    /// Get a public key for the provided ethereum address. If no key found a `None` is returned.
    pub fn get_key_by_addr(&self, address: Address) -> Result<Option<PublicKey>> {
        let keys = self.list_keys()?;

        for key in keys {
            if key.to_address() == address {
                return Ok(Some(key));
            }
        }

        Ok(None)
    }

    /// Check if key exists for the ethereum address.
    pub fn has_addr(&self, address: Address) -> Result<bool> {
        Ok(self.get_key_by_addr(address)?.is_some())
    }

    /// Check if key exists in the key store.
    pub fn has_key(&self, key: PublicKey) -> Result<bool> {
        let key_path = self.key_store.join(key.to_hex());
        let has_key = fs::metadata(key_path).is_ok();
        Ok(has_key)
    }

    /// Add a private key to the key store.
    pub fn add_key(&self, key: PrivateKey) -> Result<PublicKey> {
        let public_key: PublicKey = key.into();

        let key_file = self.key_store.join(public_key.to_hex());
        fs::write(key_file, key.0)?;

        Ok(public_key)
    }

    /// Generate a new private key and return a public key for it.
    pub fn generate_key(&self) -> Result<PublicKey> {
        let (secp256k1_secret_key, secp256k1_public_key) =
            secp256k1::generate_keypair(&mut secp256k1::rand::thread_rng());

        let public_key: PublicKey = secp256k1_public_key.into();

        let key_file = self.key_store.join(public_key.to_hex());
        println!(
            "Secret key: {}",
            secp256k1_secret_key
                .secret_bytes()
                .to_hex_string(Case::Lower)
        );
        fs::write(key_file, secp256k1_secret_key.secret_bytes())?;

        Ok(public_key)
    }

    /// Remove all the keys from the key store.
    pub fn clear_keys(&self) -> Result<()> {
        fs::remove_dir_all(&self.key_store)?;

        Ok(())
    }

    /// Get a list of the stored public keys.
    pub fn list_keys(&self) -> Result<Vec<PublicKey>> {
        let mut keys = vec![];

        for entry in fs::read_dir(&self.key_store)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let key = PublicKey::from_str(file_name.to_string_lossy().as_ref())?;
            keys.push(key);
        }

        Ok(keys)
    }

    /// Get a private key for the public one from the key store.
    pub fn get_private_key(&self, key: PublicKey) -> Result<PrivateKey> {
        let mut buf = [0u8; 32];

        let key_path = self.key_store.join(key.to_hex());
        let bytes = fs::read(key_path)?;

        if bytes.len() != 32 {
            bail!("Invalid key length: {:?}", bytes);
        }

        buf.copy_from_slice(&bytes);

        Ok(PrivateKey(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{keccak256, PrimitiveSignature as AlloySignature};
    use gprimitives::ActorId;
    use std::env::temp_dir;

    #[test]
    fn test_signer_with_known_vectors() {
        // Known test vector data
        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";

        let message = b"hello world";

        // Create the signer with a temporary key store path
        let key_store = PathBuf::from("/tmp/key-store-test-vectors");
        let signer = Signer::new(key_store.clone()).expect("Failed to create signer");

        // Convert the private key hex to bytes and add it to the signer
        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer.add_key(private_key).expect("Failed to add key");

        // Ensure the key store has the key
        assert!(signer.has_key(public_key).unwrap());

        // Sign the message
        let signature = signer
            .sign(public_key, message)
            .expect("Failed to sign message");

        // Hash the message using Keccak256
        let hash = keccak256(message);

        // Recover the address using the signature
        let alloy_sig = AlloySignature::try_from(signature.as_ref()).expect("failed to parse sig");

        let recovered_address = alloy_sig
            .recover_address_from_prehash(&hash)
            .expect("Failed to recover address");

        // Verify the recovered address matches the expected address
        assert_eq!(
            format!("{:?}", recovered_address),
            format!("{}", public_key.to_address())
        );

        // Clean up the key store directory
        signer.clear_keys().unwrap();
    }

    #[test]
    fn test_signer_with_addr() {
        // Create the signer with a temporary key store path
        let key_store = PathBuf::from("/tmp/key-store-test-addr");
        let signer = Signer::new(key_store.clone()).expect("Failed to create signer");

        // Generate a new key
        let public_key = signer.generate_key().expect("Failed to generate key");

        // Ensure the key store has the key
        assert!(signer.has_key(public_key).unwrap());

        // Sign the message
        let message = b"hello world";
        let signature = signer
            .sign_with_addr(public_key.to_address(), message)
            .expect("Failed to sign message");

        // Hash the message using Keccak256
        let hash = keccak256(message);

        // Recover the address using the signature
        let alloy_sig = AlloySignature::try_from(signature.as_ref()).expect("failed to parse sig");

        let recovered_address = alloy_sig
            .recover_address_from_prehash(&hash)
            .expect("Failed to recover address");

        // Verify the recovered address matches the expected address
        assert_eq!(
            format!("{:?}", recovered_address),
            format!("{}", public_key.to_address())
        );

        // Clean up the key store directory
        signer.clear_keys().unwrap();
    }

    #[test]
    fn try_from_actor_id() {
        let id =
            ActorId::from_str("0x0000000000000000000000006e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect("Must be correct ethereum address");

        let id =
            ActorId::from_str("0x1111111111111111111111116e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect_err("Must be incorrect ethereum address");
    }

    #[test]
    fn recover_digest() {
        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";
        let message = b"hello world";

        let key_store = temp_dir().join("signer-tests");
        let signer = Signer::new(key_store).expect("Failed to create signer");

        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer.add_key(private_key).expect("Failed to add key");

        let signature = signer
            .sign(public_key, message)
            .expect("Failed to sign message");

        let hash = keccak256(message).0;

        let recovered_public_key = signature
            .recover_from_digest(hash.into())
            .expect("Failed to recover public key");

        assert_eq!(recovered_public_key, public_key);
    }
}
