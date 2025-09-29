// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use anyhow::{Result, anyhow, bail};
use gearexe_common::{
    Address,
    ecdsa::{PrivateKey, PublicKey},
};
use std::{collections::BTreeMap, fmt, fs, path::PathBuf, str::FromStr};
use tempfile::TempDir;

pub trait KeyStorage: fmt::Debug + Send + Sync + 'static {
    /// Create an empty key store.
    fn empty() -> Self
    where
        Self: Sized;

    /// Get a private key for the public one from the key store.
    fn get_private_key(&self, key: PublicKey) -> Result<PrivateKey>;

    /// Get a public key for the provided ethereum address. If no key found a `None` is returned.
    ///
    /// Note: Could be very expensive cause bypasses all keys in storage.
    fn get_key_by_addr(&self, address: Address) -> Result<Option<PublicKey>> {
        self.list_keys()
            .map(|keys| keys.into_iter().find(|key| key.to_address() == address))
    }

    /// Add a private key to the key store.
    fn add_key(&mut self, key: PrivateKey) -> Result<PublicKey>;

    /// Check if key exists in the key store.
    fn has_key(&self, key: PublicKey) -> Result<bool>;

    /// Check if key exists for the ethereum address.
    ///
    /// Note: Could be very expensive cause bypasses all keys in storage.
    fn has_addr(&self, address: Address) -> Result<bool> {
        self.list_keys()
            .map(|keys| keys.into_iter().any(|key| key.to_address() == address))
    }

    /// List all keys in the key store.
    fn list_keys(&self) -> Result<Vec<PublicKey>>;

    /// Remove all the keys from the key store.
    fn clear_keys(&mut self) -> Result<()>;
}

#[derive(derive_more::Debug)]
pub struct FSKeyStorage {
    pub path: PathBuf,

    #[debug(skip)]
    pub _tmp_dir: Option<TempDir>,
}

impl FSKeyStorage {
    pub fn from_path(path: PathBuf) -> Self {
        Self {
            path,
            _tmp_dir: None,
        }
    }

    pub fn tmp() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");

        FSKeyStorage {
            path: temp_dir.path().to_path_buf(),
            _tmp_dir: Some(temp_dir),
        }
    }
}

impl KeyStorage for FSKeyStorage {
    fn empty() -> Self {
        Self::tmp()
    }

    fn get_private_key(&self, key: PublicKey) -> Result<PrivateKey> {
        let mut buf = [0u8; 32];

        let key_path = self.path.join(key.to_hex());
        let bytes = fs::read(key_path)?;

        if bytes.len() != 32 {
            bail!("Invalid key length: {:?}", bytes);
        }

        buf.copy_from_slice(&bytes);

        Ok(PrivateKey::from(buf))
    }

    fn add_key(&mut self, key: PrivateKey) -> Result<PublicKey> {
        let public_key: PublicKey = key.into();
        let key_file = self.path.join(public_key.to_hex());
        fs::write(key_file, <[u8; 32]>::from(key))?;
        Ok(public_key)
    }

    fn has_key(&self, key: PublicKey) -> Result<bool> {
        let key_path = self.path.join(key.to_hex());
        let has_key = fs::metadata(key_path).is_ok();
        Ok(has_key)
    }

    fn list_keys(&self) -> Result<Vec<PublicKey>> {
        let mut keys = vec![];

        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let key = PublicKey::from_str(file_name.to_string_lossy().as_ref())?;
            keys.push(key);
        }

        Ok(keys)
    }

    fn clear_keys(&mut self) -> Result<()> {
        fs::remove_dir_all(&self.path)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default, derive_more::From)]
pub struct MemoryKeyStorage {
    keys: BTreeMap<PublicKey, PrivateKey>,
}

impl KeyStorage for MemoryKeyStorage {
    fn empty() -> Self {
        Self::default()
    }

    fn get_private_key(&self, key: PublicKey) -> Result<PrivateKey> {
        self.keys
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("Key not found"))
    }

    fn add_key(&mut self, key: PrivateKey) -> Result<PublicKey> {
        let public_key: PublicKey = key.into();
        self.keys.insert(public_key, key);
        Ok(public_key)
    }

    fn has_key(&self, key: PublicKey) -> Result<bool> {
        Ok(self.keys.contains_key(&key))
    }

    fn list_keys(&self) -> Result<Vec<PublicKey>> {
        Ok(self.keys.keys().cloned().collect())
    }

    fn clear_keys(&mut self) -> Result<()> {
        self.keys.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_key_storage() {
        test_all::<FSKeyStorage>();
    }

    #[test]
    fn test_memory_key_storage() {
        test_all::<MemoryKeyStorage>();
    }

    fn test_all<S: KeyStorage>() {
        test_add_and_get_key(S::empty());
        test_has_key(S::empty());
        test_has_addr(S::empty());
        test_list_keys(S::empty());
        test_clear_keys(S::empty());
    }

    fn test_add_and_get_key<S: KeyStorage>(mut storage: S) {
        let private_key = PrivateKey::from([1u8; 32]);
        let public_key = storage.add_key(private_key).unwrap();

        assert_eq!(storage.get_private_key(public_key).unwrap(), private_key);
    }

    fn test_has_key<S: KeyStorage>(mut storage: S) {
        let private_key = PrivateKey::from([2u8; 32]);
        let public_key: PublicKey = private_key.into();
        assert!(!storage.has_key(public_key).unwrap());

        assert_eq!(public_key, storage.add_key(private_key).unwrap());
        assert!(storage.has_key(public_key).unwrap());
    }

    fn test_list_keys<S: KeyStorage>(mut storage: S) {
        let private_key1 = PrivateKey::from([3u8; 32]);
        let private_key2 = PrivateKey::from([4u8; 32]);
        let public_key1 = storage.add_key(private_key1).unwrap();
        let public_key2 = storage.add_key(private_key2).unwrap();

        let keys = storage.list_keys().unwrap();
        assert!(keys.contains(&public_key1));
        assert!(keys.contains(&public_key2));
    }

    fn test_clear_keys<S: KeyStorage>(mut storage: S) {
        let private_key = PrivateKey::from([5u8; 32]);
        let public_key = storage.add_key(private_key).unwrap();

        assert!(storage.has_key(public_key).unwrap());

        storage.clear_keys().unwrap();
        assert!(!storage.has_key(public_key).unwrap());
    }

    fn test_has_addr<S: KeyStorage>(mut storage: S) {
        let private_key = PrivateKey::from([6u8; 32]);
        let public_key: PublicKey = private_key.into();
        let address = public_key.to_address();

        assert!(!storage.has_addr(address).unwrap());
        storage.add_key(private_key).unwrap();
        assert!(storage.has_addr(address).unwrap());
    }
}
