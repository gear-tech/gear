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

//! Unified keyring manager supporting multiple cryptographic schemes.
//!
//! This module provides a top-level keyring abstraction that can manage keys
//! across different signature schemes by relying on scheme-specific keystore
//! types to implement [`KeystoreEntry`].

pub mod simple;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const CONFIG_FILE: &str = "keyring.json";

/// Trait for keystore types that can be used with the keyring.
pub trait KeystoreEntry: Serialize + for<'de> Deserialize<'de> + Clone {
    /// Get the name/identifier of this keystore entry.
    fn name(&self) -> &str;

    /// Set the name of this keystore entry.
    fn set_name(&mut self, name: &str);
}

/// Keyring configuration stored on disk.
#[derive(Default, Serialize, Deserialize)]
struct KeyringConfig {
    /// The primary key name (if set).
    primary: Option<String>,
}

/// Unified keyring manager for cryptographic keys.
///
/// Manages a collection of serialized keystores with a primary key concept.
/// The keystore format is delegated to the scheme-specific implementation via
/// the [`KeystoreEntry`] trait.
///
/// # Directory Structure
///
/// ```text
/// keyring/
/// ├── keyring.json          # Configuration (primary key)
/// ├── alice.json            # Individual keystores
/// ├── bob.json
/// └── ...
/// ```
pub struct Keyring<K: KeystoreEntry> {
    /// Path to the keyring directory.
    store: PathBuf,
    /// Loaded keystores.
    keystores: Vec<K>,
    /// Primary key name.
    primary: Option<String>,
}

impl<K: KeystoreEntry> Keyring<K> {
    /// Load keyring from directory.
    ///
    /// Creates the directory if it doesn't exist and loads all keystores from disk.
    pub fn load(store: PathBuf) -> Result<Self> {
        fs::create_dir_all(&store)?;

        let mut keystores = Vec::new();
        for entry in fs::read_dir(&store)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::warn!("Failed to iterate keyring directory: {err}");
                    continue;
                }
            };

            let path = entry.path();
            if Self::is_config_file(&path) || !Self::is_keystore_file(&path) {
                continue;
            }

            match Self::read_keystore(&path) {
                Ok(keystore) => keystores.push(keystore),
                Err(err) => tracing::warn!("Failed to load keystore at {:?}: {err}", path),
            }
        }

        let config_path = store.join(CONFIG_FILE);
        let primary = if config_path.exists() {
            let config: KeyringConfig = serde_json::from_slice(&fs::read(&config_path)?)?;
            config.primary
        } else {
            None
        };

        Ok(Self {
            store,
            keystores,
            primary,
        })
    }

    fn is_config_file(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == CONFIG_FILE)
    }

    fn is_keystore_file(path: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "json")
    }

    fn read_keystore(path: &Path) -> Result<K> {
        let bytes = fs::read(path)?;
        let mut keystore: K = serde_json::from_slice(&bytes)?;

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && keystore.name().is_empty()
        {
            keystore.set_name(stem);
        }

        Ok(keystore)
    }

    fn keystore_path(&self, name: &str) -> PathBuf {
        self.store.join(name).with_extension("json")
    }

    /// Save keyring configuration to disk.
    fn save_config(&self) -> Result<()> {
        let config = KeyringConfig {
            primary: self.primary.clone(),
        };
        let path = self.store.join(CONFIG_FILE);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        Ok(())
    }

    /// Persist a keystore entry in the keyring.
    ///
    /// Saves the keystore to disk, overwriting any existing entry with the same name.
    pub fn store(&mut self, name: &str, mut keystore: K) -> Result<K> {
        keystore.set_name(name);

        let path = self.keystore_path(name);
        fs::write(&path, serde_json::to_vec_pretty(&keystore)?)?;

        if let Some(index) = self.keystores.iter().position(|entry| entry.name() == name) {
            self.keystores[index] = keystore.clone();
        } else {
            self.keystores.push(keystore.clone());
        }

        Ok(keystore)
    }

    /// Import a keystore from an arbitrary JSON file.
    ///
    /// The file is deserialized, optionally renamed from its filename, and stored
    /// in the keyring directory.
    pub fn import(&mut self, path: PathBuf) -> Result<K> {
        let mut keystore = Self::read_keystore(&path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?;

        keystore.set_name(name);
        self.store(name, keystore)
    }

    /// Get the primary keystore.
    ///
    /// Returns an error if no primary key is set or if the keyring is empty.
    pub fn primary(&mut self) -> Result<&K> {
        if self.keystores.is_empty() {
            return Err(anyhow!("No keys in keyring"));
        }

        if self.primary.is_none() {
            let first = self.keystores[0].name().to_string();
            self.primary = Some(first);
            self.save_config()?;
        }

        let primary_name = self.primary.as_ref().unwrap();
        self.keystores
            .iter()
            .find(|k| k.name() == primary_name)
            .ok_or_else(|| anyhow!("Primary key '{}' not found in keyring", primary_name))
    }

    /// Set the primary key by name.
    pub fn set_primary(&mut self, name: &str) -> Result<()> {
        if !self.keystores.iter().any(|k| k.name() == name) {
            return Err(anyhow!("Key '{}' not found in keyring", name));
        }

        self.primary = Some(name.to_string());
        self.save_config()?;
        Ok(())
    }

    /// List all keystores in the keyring.
    pub fn list(&self) -> &[K] {
        &self.keystores
    }

    /// Get a keystore by name.
    pub fn get(&self, name: &str) -> Option<&K> {
        self.keystores.iter().find(|k| k.name() == name)
    }

    /// Remove a keystore by name.
    pub fn remove(&mut self, name: &str) -> Result<K> {
        let index = self
            .keystores
            .iter()
            .position(|k| k.name() == name)
            .ok_or_else(|| anyhow!("Key '{}' not found", name))?;

        let keystore = self.keystores.remove(index);

        // Remove from disk
        let path = self.keystore_path(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Clear primary if it was the removed key
        if self.primary.as_deref() == Some(name) {
            self.primary = None;
            self.save_config()?;
        }

        Ok(keystore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;

    #[derive(Clone, Serialize, Deserialize)]
    struct TestKeystore {
        name: String,
        data: String,
    }

    impl KeystoreEntry for TestKeystore {
        fn name(&self) -> &str {
            &self.name
        }

        fn set_name(&mut self, name: &str) {
            self.name = name.to_string();
        }
    }

    #[test]
    fn test_keyring_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut keyring = Keyring::<TestKeystore>::load(temp_dir.path().to_path_buf()).unwrap();

        // Add keystores
        let key1 = TestKeystore {
            name: String::new(),
            data: "secret1".to_string(),
        };
        let key2 = TestKeystore {
            name: String::from("bob"),
            data: "secret2".to_string(),
        };

        keyring.store("alice", key1).unwrap();
        keyring.store("bob", key2).unwrap();

        // List
        assert_eq!(keyring.list().len(), 2);
        assert_eq!(
            keyring
                .list()
                .iter()
                .map(|k| k.name())
                .collect::<HashSet<_>>(),
            HashSet::from(["alice", "bob"])
        );

        // Get
        assert!(keyring.get("alice").is_some());
        assert!(keyring.get("charlie").is_none());

        // Set primary
        keyring.set_primary("alice").unwrap();
        assert_eq!(keyring.primary.as_deref(), Some("alice"));
        keyring.primary().unwrap();

        // Remove
        keyring.remove("alice").unwrap();
        assert_eq!(keyring.list().len(), 1);
        assert!(keyring.primary.is_none());
    }
}
