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

//! Unified keyring manager supporting multiple cryptographic schemes.
//!
//! This module provides a top-level keyring abstraction that can manage keys
//! across different signature schemes by relying on scheme-specific keystore
//! types to implement [`KeystoreEntry`].

pub mod encryption;
pub mod key_codec;
mod scheme;
pub use scheme::KeyringScheme;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const CONFIG_FILE: &str = "keyring.json";

pub const NAMESPACE_NET: &str = "net";
pub const NAMESPACE_SECP: &str = "secp";
pub const NAMESPACE_ED: &str = "ed";
pub const NAMESPACE_SR: &str = "sr";

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
trait KeyringBackend: Send + Sync {
    fn list_entries(&self) -> Result<Vec<(String, Vec<u8>)>>;
    fn read_config(&self) -> Result<Option<Vec<u8>>>;
    fn write_config(&mut self, bytes: &[u8]) -> Result<()>;
    fn write_entry(&mut self, name: &str, bytes: &[u8]) -> Result<()>;
    fn remove_entry(&mut self, name: &str) -> Result<()>;
}

struct DiskBackend {
    root: PathBuf,
}

impl DiskBackend {
    fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn entry_path(&self, name: &str) -> PathBuf {
        self.root.join(name).with_extension("json")
    }
}

impl KeyringBackend for DiskBackend {
    fn list_entries(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut entries = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::warn!("Failed to iterate keyring directory: {err}");
                    continue;
                }
            };

            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|ext| ext == "json")
                && path.file_name().is_none_or(|name| name != CONFIG_FILE)
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                match fs::read(&path) {
                    Ok(bytes) => entries.push((name.to_string(), bytes)),
                    Err(err) => tracing::warn!("Failed to read keystore at {:?}: {err}", path),
                }
            }
        }

        Ok(entries)
    }

    fn read_config(&self) -> Result<Option<Vec<u8>>> {
        let config_path = self.root.join(CONFIG_FILE);
        if config_path.exists() {
            Ok(Some(fs::read(config_path)?))
        } else {
            Ok(None)
        }
    }

    fn write_config(&mut self, bytes: &[u8]) -> Result<()> {
        fs::write(self.root.join(CONFIG_FILE), bytes)?;
        Ok(())
    }

    fn write_entry(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        fs::write(self.entry_path(name), bytes)?;
        Ok(())
    }

    fn remove_entry(&mut self, name: &str) -> Result<()> {
        let file = self.entry_path(name);
        if file.exists() {
            fs::remove_file(file)?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct MemoryBackend {
    keystores: HashMap<String, Vec<u8>>,
    config: Option<Vec<u8>>,
}

impl KeyringBackend for MemoryBackend {
    fn list_entries(&self) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(self
            .keystores
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect())
    }

    fn read_config(&self) -> Result<Option<Vec<u8>>> {
        Ok(self.config.clone())
    }

    fn write_config(&mut self, bytes: &[u8]) -> Result<()> {
        self.config = Some(bytes.to_vec());
        Ok(())
    }

    fn write_entry(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.keystores.insert(name.to_string(), bytes.to_vec());
        Ok(())
    }

    fn remove_entry(&mut self, name: &str) -> Result<()> {
        self.keystores.remove(name);
        Ok(())
    }
}

pub struct Keyring<K: KeystoreEntry> {
    store: Box<dyn KeyringBackend>,
    keystores: Vec<K>,
    primary: Option<String>,
}

fn resolve_namespaced_path_impl(store: PathBuf, namespace: &str) -> PathBuf {
    if path_has_keyring(&store) || store.file_name().is_some_and(|name| name == namespace) {
        store
    } else {
        store.join(namespace)
    }
}

/// Resolve a storage path into a namespaced keyring directory.
///
/// This helper can be used without instantiating a [`Keyring`] to compute the
/// scheme-specific directory that should back the JSON keystore.
pub fn resolve_namespaced_path(store: PathBuf, namespace: &str) -> PathBuf {
    resolve_namespaced_path_impl(store, namespace)
}

impl<K: KeystoreEntry> Keyring<K> {
    /// Load keyring from directory.
    ///
    /// Creates the directory if it doesn't exist and loads all keystores from disk.
    pub fn load(store: PathBuf) -> Result<Self> {
        let backend = Box::new(DiskBackend::new(store)?);
        Self::from_backend(backend)
    }

    /// Create an in-memory keyring.
    pub fn memory() -> Self {
        Self::try_memory().unwrap_or_else(|_| Self {
            store: Box::new(MemoryBackend::default()),
            keystores: Vec::new(),
            primary: None,
        })
    }

    /// Fallible constructor for an in-memory keyring.
    pub fn try_memory() -> Result<Self> {
        Self::from_backend(Box::new(MemoryBackend::default()))
    }

    fn from_backend(store: Box<dyn KeyringBackend>) -> Result<Self> {
        let mut keystores = Vec::new();
        for (name, bytes) in store.list_entries()? {
            match Self::decode_keystore(&bytes, Some(&name)) {
                Ok(keystore) => keystores.push(keystore),
                Err(err) => tracing::warn!("Failed to load keystore '{name}': {err}"),
            }
        }

        let primary = if let Some(config_bytes) = store.read_config()? {
            let config: KeyringConfig = serde_json::from_slice(&config_bytes)?;
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

    /// Resolve a storage path into a namespaced keyring directory.
    ///
    /// This allows callers to pass a common root (e.g. `/keys`) while keeping
    /// scheme-specific keyrings separate (`/keys/secp`, `/keys/ed`, `/keys/net`, ...).
    /// If the provided path already contains keystores or configuration, it is returned
    /// unchanged for backward compatibility.
    pub fn namespaced_path(store: PathBuf, namespace: &str) -> PathBuf {
        resolve_namespaced_path_impl(store, namespace)
    }

    fn decode_keystore(bytes: &[u8], inferred_name: Option<&str>) -> Result<K> {
        let mut keystore: K = serde_json::from_slice(bytes)?;

        if let Some(stem) = inferred_name
            && keystore.name().is_empty()
        {
            keystore.set_name(stem);
        }

        Ok(keystore)
    }

    fn read_keystore_from_path(path: &Path) -> Result<K> {
        let bytes = fs::read(path)?;
        let inferred = path.file_stem().and_then(|s| s.to_str());
        Self::decode_keystore(&bytes, inferred)
    }

    /// Save keyring configuration to disk.
    fn save_config(&mut self) -> Result<()> {
        let config = KeyringConfig {
            primary: self.primary.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&config)?;
        self.store.write_config(&bytes)?;
        Ok(())
    }

    /// Persist a keystore entry in the keyring.
    ///
    /// Saves the keystore to disk, overwriting any existing entry with the same name.
    pub fn store(&mut self, name: &str, mut keystore: K) -> Result<K> {
        keystore.set_name(name);

        let bytes = serde_json::to_vec_pretty(&keystore)?;
        self.store.write_entry(name, &bytes)?;

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
        let mut keystore = Self::read_keystore_from_path(&path)?;
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

        let primary_name = self
            .primary
            .as_ref()
            .ok_or_else(|| anyhow!("Primary key is not set"))?;
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

        // Remove from storage backend
        self.store.remove_entry(name)?;

        // Clear primary if it was the removed key
        if self.primary.as_deref() == Some(name) {
            self.primary = None;
            self.save_config()?;
        }

        Ok(keystore)
    }
}

fn path_has_keyring(path: &Path) -> bool {
    if path.join(CONFIG_FILE).exists() {
        return true;
    }

    fs::read_dir(path)
        .map(|entries| {
            entries.flatten().any(|entry| {
                let file_path = entry.path();
                file_path.is_file() && file_path.extension().is_some_and(|ext| ext == "json")
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::{collections::HashSet, fs};

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

    #[test]
    fn namespaced_path_defaults_to_namespace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        fs::create_dir_all(&root).unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root.clone(), NAMESPACE_SECP);
        assert_eq!(resolved, root.join(NAMESPACE_SECP));
    }

    #[test]
    fn namespaced_path_preserves_existing_store() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("alice.json"), "{}").unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root.clone(), NAMESPACE_ED);
        assert_eq!(resolved, root);
    }

    #[test]
    fn namespaced_path_prefers_existing_namespace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("keys");
        let namespaced = root.join(NAMESPACE_NET);
        fs::create_dir_all(&namespaced).unwrap();
        fs::write(namespaced.join("alice.json"), "{}").unwrap();

        let resolved = Keyring::<TestKeystore>::namespaced_path(root, NAMESPACE_NET);
        assert!(resolved.ends_with(NAMESPACE_NET));
        assert!(resolved.join("alice.json").exists());
    }
}
