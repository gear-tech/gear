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

//! Unified signer API backed by the JSON keyring.

#![cfg(all(feature = "std", feature = "keyring", feature = "serde"))]

use crate::{
    error::{Result, SignerError},
    keyring::{self, KeyringScheme, KeystoreEntry},
};
use hex::ToHex;
use std::{
    fmt,
    marker::PhantomData,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tempfile::TempDir;

/// Universal signer that works with any signature scheme by storing keys in the keyring.
#[derive(Clone)]
pub struct Signer<S: KeyringScheme> {
    keyring: Arc<RwLock<keyring::Keyring<S::Keystore>>>,
    _tmp_dir: Option<Arc<TempDir>>,
    _phantom: PhantomData<S>,
}

impl<S: KeyringScheme> fmt::Debug for Signer<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer")
            .field("scheme", &S::scheme_name())
            .field("keys", &self.list_keys().ok())
            .finish()
    }
}

impl<S: KeyringScheme> Signer<S> {
    /// Create a signer backed by the provided keyring.
    pub fn new(keyring: keyring::Keyring<S::Keystore>) -> Self {
        Self {
            keyring: Arc::new(RwLock::new(keyring)),
            _tmp_dir: None,
            _phantom: PhantomData,
        }
    }

    fn with_tempdir(keyring: keyring::Keyring<S::Keystore>, tmp_dir: Option<TempDir>) -> Self {
        Self {
            keyring: Arc::new(RwLock::new(keyring)),
            _tmp_dir: tmp_dir.map(Arc::new),
            _phantom: PhantomData,
        }
    }

    /// Create a signer with an in-memory keyring.
    pub fn memory() -> Self {
        let keyring = keyring::Keyring::try_memory().expect("memory keyring should not fail");
        Self::new(keyring)
    }

    fn namespaced_path(path: PathBuf) -> PathBuf {
        keyring::Keyring::<S::Keystore>::namespaced_path(path, S::namespace())
    }

    /// Create a signer backed by a filesystem keyring at the specified path.
    /// Returns an error if the keyring cannot be loaded.
    pub fn fs(path: PathBuf) -> Result<Self> {
        let namespaced = Self::namespaced_path(path);
        let keyring = keyring::Keyring::load(namespaced)?;
        Ok(Self::new(keyring))
    }

    /// Create a signer backed by a temporary filesystem keyring.
    pub fn fs_temporary() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let namespaced = Self::namespaced_path(temp_dir.path().to_path_buf());
        let keyring = keyring::Keyring::load(namespaced)?;
        Ok(Self::with_tempdir(keyring, Some(temp_dir)))
    }

    fn keyring(&self) -> Result<RwLockReadGuard<'_, keyring::Keyring<S::Keystore>>> {
        self.keyring
            .read()
            .map_err(|err| SignerError::Other(format!("Failed to acquire read lock: {err}")))
    }

    fn keyring_mut(&self) -> Result<RwLockWriteGuard<'_, keyring::Keyring<S::Keystore>>> {
        self.keyring
            .write()
            .map_err(|err| SignerError::Other(format!("Failed to acquire write lock: {err}")))
    }

    fn key_name(public_key: &S::PublicKey) -> String {
        format!(
            "key-{}",
            S::public_key_bytes(public_key).encode_hex::<String>()
        )
    }

    fn store_private_key(
        &self,
        private_key: S::PrivateKey,
        password: Option<&str>,
    ) -> Result<S::PublicKey> {
        let public_key = S::public_key(&private_key);
        let name = Self::key_name(&public_key);
        let keystore = S::keystore_from_private(&name, &private_key, password)?;
        self.keyring_mut()?.store(&name, keystore)?;
        Ok(public_key)
    }

    fn with_keystore<F, R>(&self, public_key: &S::PublicKey, mut f: F) -> Result<R>
    where
        F: FnMut(&S::Keystore) -> Result<R>,
    {
        let storage = self.keyring()?;
        for keystore in storage.list() {
            if S::keystore_public(keystore)? == *public_key {
                return f(keystore);
            }
        }
        Err(SignerError::KeyNotFound(format!("{public_key:?}")))
    }

    /// Generate a new keypair and store it.
    pub fn generate_key(&self) -> Result<S::PublicKey> {
        self.generate_key_with_password(None)
    }

    /// Generate a new keypair and store it with the provided password.
    pub fn generate_key_with_password(&self, password: Option<&str>) -> Result<S::PublicKey> {
        let (private_key, _) = S::generate_keypair();
        self.store_private_key(private_key, password)
    }

    /// Import an existing private key.
    pub fn import_key(&self, private_key: S::PrivateKey) -> Result<S::PublicKey> {
        self.import_key_with_password(private_key, None)
    }

    /// Import an existing private key with the provided password.
    pub fn import_key_with_password(
        &self,
        private_key: S::PrivateKey,
        password: Option<&str>,
    ) -> Result<S::PublicKey> {
        self.store_private_key(private_key, password)
    }

    /// Sign data with the specified public key.
    pub fn sign(&self, public_key: S::PublicKey, data: &[u8]) -> Result<S::Signature> {
        self.sign_with_password(public_key, data, None)
    }

    /// Sign data with the specified public key using the provided password.
    pub fn sign_with_password(
        &self,
        public_key: S::PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<S::Signature> {
        let private_key = self.get_private_key_with_password(public_key, password)?;
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

    /// Check if a key exists in storage.
    pub fn has_key(&self, public_key: S::PublicKey) -> Result<bool> {
        let storage = self.keyring()?;
        for keystore in storage.list() {
            if S::keystore_public(keystore)? == public_key {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// List all public keys in storage.
    pub fn list_keys(&self) -> Result<Vec<S::PublicKey>> {
        let storage = self.keyring()?;
        storage.list().iter().map(S::keystore_public).collect()
    }

    /// Remove all keys from storage.
    pub fn clear_keys(&self) -> Result<()> {
        let mut storage = self.keyring_mut()?;
        let names: Vec<String> = storage
            .list()
            .iter()
            .map(|keystore| keystore.name().to_string())
            .collect();
        for name in names {
            storage.remove(&name)?;
        }
        Ok(())
    }

    /// Create a sub-signer with a subset of keys.
    pub fn sub_signer(&self, keys: Vec<S::PublicKey>) -> Result<Self> {
        self.sub_signer_with_password(keys, None)
    }

    /// Create a sub-signer with a subset of keys using the provided password.
    pub fn sub_signer_with_password(
        &self,
        keys: Vec<S::PublicKey>,
        password: Option<&str>,
    ) -> Result<Self> {
        let signer = Signer::memory();
        for key in keys {
            let private_key = self.get_private_key_with_password(key.clone(), password)?;
            signer.import_key_with_password(private_key, password)?;
        }
        Ok(signer)
    }

    /// Get the scheme name.
    pub fn scheme_name(&self) -> &'static str {
        S::scheme_name()
    }

    /// Get a private key by public key.
    pub fn get_private_key(&self, public_key: S::PublicKey) -> Result<S::PrivateKey> {
        self.get_private_key_with_password(public_key, None)
    }

    /// Get a private key by public key using the provided password.
    pub fn get_private_key_with_password(
        &self,
        public_key: S::PublicKey,
        password: Option<&str>,
    ) -> Result<S::PrivateKey> {
        self.with_keystore(&public_key, |keystore| {
            S::keystore_private(keystore, password)
        })
    }

    /// Try to find a public key associated with the provided address.
    pub fn get_key_by_address(&self, address: S::Address) -> Result<Option<S::PublicKey>> {
        let storage = self.keyring()?;
        for keystore in storage.list() {
            if S::keystore_address(keystore)? == address {
                return S::keystore_public(keystore).map(Some);
            }
        }
        Ok(None)
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

        let public_key = signer.generate_key().unwrap();
        assert!(signer.has_key(public_key).unwrap());

        let message = b"hello world";
        let signature = signer.sign(public_key, message).unwrap();
        signer.verify(public_key, message, &signature).unwrap();
    }
}
