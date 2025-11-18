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

//! Unified signer API.

use crate::{
    error::Result,
    storage::{FSKeyStorage, MemoryKeyStorage},
    traits::{KeyStorage, SignatureScheme},
};
use std::{
    fs,
    marker::PhantomData,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Universal signer that works with any signature scheme.
///
/// The signer provides a unified interface for cryptographic operations
/// while maintaining scheme-specific behavior through the `SignatureScheme` trait.
#[derive(Clone)]
pub struct Signer<S: SignatureScheme> {
    storage: Arc<RwLock<dyn KeyStorage<S>>>,
    _phantom: PhantomData<S>,
}

impl<S: SignatureScheme> Signer<S> {
    /// Create a new signer with the provided key storage.
    pub fn new<K: KeyStorage<S>>(storage: K) -> Self {
        Self {
            storage: Arc::new(RwLock::new(storage)),
            _phantom: PhantomData,
        }
    }

    /// Create a signer with filesystem storage at the specified path.
    pub fn fs(path: PathBuf) -> Self
    where
        S::PrivateKey: crate::traits::SeedableKey,
    {
        fs::create_dir_all(&path).expect("Failed to create storage directory");
        Self::new(FSKeyStorage::<S>::from_path(path))
    }

    /// Create a signer with temporary filesystem storage.
    pub fn fs_temporary() -> Self
    where
        S::PrivateKey: crate::traits::SeedableKey,
    {
        Self::new(FSKeyStorage::<S>::tmp())
    }

    /// Create a signer with in-memory storage.
    pub fn memory() -> Self {
        Self::new(MemoryKeyStorage::<S>::new())
    }

    /// Generate a new keypair and store it.
    pub fn generate_key(&self) -> Result<S::PublicKey> {
        let (private_key, _) = S::generate_keypair();
        self.storage_mut().add_key(private_key)
    }

    /// Sign data with the specified public key.
    pub fn sign(&self, public_key: S::PublicKey, data: &[u8]) -> Result<S::Signature> {
        let private_key = self.storage().get_private_key(public_key)?;
        S::sign(&private_key, data)
    }

    /// Verify a signature.
    pub fn verify(
        &self,
        public_key: S::PublicKey,
        data: &[u8],
        signature: &S::Signature,
    ) -> Result<()> {
        S::verify(&public_key, data, signature)
    }

    /// Get the address for a public key.
    pub fn address(&self, public_key: S::PublicKey) -> S::Address {
        S::address(&public_key)
    }

    /// Import an existing private key.
    pub fn import_key(&self, private_key: S::PrivateKey) -> Result<S::PublicKey> {
        self.storage_mut().add_key(private_key)
    }

    /// Check if a key exists in storage.
    pub fn has_key(&self, public_key: S::PublicKey) -> Result<bool> {
        self.storage().has_key(public_key)
    }

    /// List all public keys in storage.
    pub fn list_keys(&self) -> Result<Vec<S::PublicKey>> {
        self.storage().list_keys()
    }

    /// Get read access to the underlying storage.
    pub fn storage(&self) -> RwLockReadGuard<'_, dyn KeyStorage<S>> {
        self.storage.read().expect("Failed to acquire read lock")
    }

    /// Get write access to the underlying storage.
    pub fn storage_mut(&self) -> RwLockWriteGuard<'_, dyn KeyStorage<S>> {
        self.storage.write().expect("Failed to acquire write lock")
    }

    /// Create a sub-signer with a subset of keys.
    pub fn sub_signer(&self, keys: Vec<S::PublicKey>) -> Result<Self> {
        let mut new_storage = MemoryKeyStorage::<S>::new();

        for key in keys {
            let private_key = self.storage().get_private_key(key)?;
            new_storage.add_key(private_key)?;
        }

        Ok(Self::new(new_storage))
    }

    /// Get the scheme name.
    pub fn scheme_name(&self) -> &'static str {
        S::scheme_name()
    }

    /// Get a private key by public key.
    pub fn get_private_key(&self, public_key: S::PublicKey) -> Result<S::PrivateKey> {
        self.storage().get_private_key(public_key)
    }

    /// Export a key as bytes (scheme-specific format).
    pub fn export_key(&self, public_key: S::PublicKey) -> Result<S::PrivateKey> {
        self.get_private_key(public_key)
    }

    /// Try to find a public key associated with the provided address.
    pub fn get_key_by_address(&self, address: S::Address) -> Result<Option<S::PublicKey>> {
        self.storage().get_key_by_address(address)
    }
}

impl<S: SignatureScheme> std::fmt::Debug for Signer<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signer")
            .field("scheme", &S::scheme_name())
            .field("keys", &self.list_keys().ok())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "secp256k1")]
    #[test]
    fn test_signer_secp256k1() {
        use crate::schemes::secp256k1::Secp256k1;

        let signer = Signer::<Secp256k1>::memory();

        // Generate key
        let public_key = signer.generate_key().unwrap();
        assert!(signer.has_key(public_key).unwrap());

        // Sign and verify
        let message = b"hello world";
        let signature = signer.sign(public_key, message).unwrap();
        signer.verify(public_key, message, &signature).unwrap();

        // List keys
        let keys = signer.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&public_key));

        // Get address
        let address = signer.address(public_key);
        assert_eq!(address.as_ref().len(), 20);
    }

    #[cfg(feature = "sr25519")]
    #[test]
    fn test_signer_sr25519() {
        use crate::schemes::sr25519::Sr25519;

        let signer = Signer::<Sr25519>::memory();

        // Generate key
        let public_key = signer.generate_key().unwrap();
        assert!(signer.has_key(public_key).unwrap());

        // Sign and verify
        let message = b"hello world";
        let signature = signer.sign(public_key, message).unwrap();
        signer.verify(public_key, message, &signature).unwrap();

        // List keys
        let keys = signer.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&public_key));
        // Get address
        let address = signer.address(public_key);
        assert_eq!(address.as_bytes().len(), 32);
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn test_signer_ed25519() {
        use crate::schemes::ed25519::Ed25519;

        let signer = Signer::<Ed25519>::memory();

        // Generate key
        let public_key = signer.generate_key().unwrap();
        assert!(signer.has_key(public_key).unwrap());

        // Sign and verify
        let message = b"hello world";
        let signature = signer.sign(public_key, message).unwrap();
        signer.verify(public_key, message, &signature).unwrap();

        // List keys
        let keys = signer.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&public_key));

        // Get address
        let address = signer.address(public_key);
        assert!(!address.as_ss58().is_empty());
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn sign_digest_recovers_original_key() {
        use crate::schemes::secp256k1::{Digest, Secp256k1, Secp256k1SignerExt};

        let signer = Signer::<Secp256k1>::memory();
        let public_key = signer.generate_key().unwrap();
        let digest = Digest([0x42; 32]);

        let signature = signer.sign_digest(public_key, &digest).unwrap();
        let recovered = signature.recover::<Digest>(digest).unwrap();

        assert_eq!(recovered, public_key);
    }

    #[cfg(all(feature = "secp256k1", feature = "sr25519"))]
    #[test]
    fn test_sub_signer() {
        use crate::schemes::secp256k1::Secp256k1;

        let main_signer = Signer::<Secp256k1>::memory();

        let key1 = main_signer.generate_key().unwrap();
        let key2 = main_signer.generate_key().unwrap();

        let sub_signer = main_signer.sub_signer(vec![key1]).unwrap();

        assert!(sub_signer.has_key(key1).unwrap());
        assert!(!sub_signer.has_key(key2).unwrap());
    }
}
