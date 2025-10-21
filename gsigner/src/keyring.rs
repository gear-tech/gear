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
//! for both secp256k1 (Ethereum) and sr25519 (Substrate) schemes.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

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
/// Manages a collection of encrypted keystores with a primary key concept.
/// Supports both sr25519 (Substrate) and secp256k1 (Ethereum) key types.
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
    /// Creates the directory if it doesn't exist.
    /// Loads all keystore files and reads the primary key configuration.
    pub fn load(store: PathBuf) -> Result<Self> {
        fs::create_dir_all(&store)?;

        let keystores = fs::read_dir(&store)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.file_name()? == CONFIG_FILE {
                    return None;
                }
                if path.extension()? != "json" {
                    return None;
                }

                let content = fs::read(&path).ok()?;
                serde_json::from_slice::<K>(&content)
                    .map_err(|err| {
                        tracing::warn!("Failed to load keystore at {:?}: {}", path, err);
                        err
                    })
                    .ok()
            })
            .collect::<Vec<_>>();

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

    /// Save keyring configuration to disk.
    fn save_config(&self) -> Result<()> {
        let config = KeyringConfig {
            primary: self.primary.clone(),
        };
        let path = self.store.join(CONFIG_FILE);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        Ok(())
    }

    /// Get the primary keystore.
    ///
    /// Returns an error if no primary key is set or if the keyring is empty.
    pub fn primary(&mut self) -> Result<&K> {
        if self.keystores.is_empty() {
            return Err(anyhow!("No keys in keyring"));
        }

        // If no primary set, use the first key
        if self.primary.is_none() {
            self.primary = Some(self.keystores[0].name().to_string());
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
        // Verify the key exists first
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

    /// Add a keystore to the keyring.
    ///
    /// Saves the keystore to disk and adds it to the in-memory collection.
    pub fn add(&mut self, name: &str, keystore: K) -> Result<K> {
        let path = self.store.join(name).with_extension("json");
        fs::write(&path, serde_json::to_vec_pretty(&keystore)?)?;

        self.keystores.push(keystore.clone());
        Ok(keystore)
    }

    /// Import a keystore from a file.
    pub fn import(&mut self, path: PathBuf) -> Result<K> {
        let content = fs::read(&path)?;
        let keystore: K = serde_json::from_slice(&content)?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?;

        self.add(name, keystore)
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
        let path = self.store.join(name).with_extension("json");
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

// Re-export scheme-specific keyring implementations will be added here
// as we integrate them with the unified keyring base

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

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
            name: "alice".to_string(),
            data: "secret1".to_string(),
        };
        let key2 = TestKeystore {
            name: "bob".to_string(),
            data: "secret2".to_string(),
        };

        keyring.add("alice", key1).unwrap();
        keyring.add("bob", key2).unwrap();

        // List
        assert_eq!(keyring.list().len(), 2);

        // Get
        assert!(keyring.get("alice").is_some());
        assert!(keyring.get("charlie").is_none());

        // Set primary
        keyring.set_primary("alice").unwrap();
        assert_eq!(keyring.primary.as_deref(), Some("alice"));

        // Remove
        keyring.remove("alice").unwrap();
        assert_eq!(keyring.list().len(), 1);
        assert!(keyring.primary.is_none());
    }
}
