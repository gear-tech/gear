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

//! Storage backend trait and implementations for keyring persistence.
//!
//! This module provides the [`StorageBackend`] trait which abstracts over
//! different storage mechanisms for keyring data. Built-in implementations
//! include:
//!
//! - [`MemoryBackend`] - In-memory storage (useful for testing)
//! - [`FilesystemBackend`] - File-based storage on disk
//!
//! # Custom Backends
//!
//! You can implement [`StorageBackend`] for custom storage needs:
//!
//! ```rust,ignore
//! use gsigner::storage::{StorageBackend, StorageResult};
//!
//! struct MyCloudBackend { /* ... */ }
//!
//! impl StorageBackend for MyCloudBackend {
//!     fn list_entries(&self) -> StorageResult<Vec<(String, Vec<u8>)>> {
//!         // Fetch from cloud storage
//!         todo!()
//!     }
//!     // ... other methods
//! }
//! ```

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use thiserror::Error;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Entry not found.
    #[error("Entry not found: {0}")]
    NotFound(String),

    /// Backend-specific error.
    #[error("Storage error: {0}")]
    Other(String),
}

/// Result type for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Trait for keyring storage backends.
///
/// This trait abstracts over different storage mechanisms, allowing the keyring
/// to work with filesystem storage, in-memory storage, or custom backends
/// (e.g., cloud storage, HSM, encrypted containers).
///
/// # Entry Format
///
/// Entries are stored as raw bytes. The keyring layer handles serialization
/// of keystore entries to/from JSON. Entry names are unique identifiers
/// (typically derived from the keystore name).
///
/// # Thread Safety
///
/// Implementations must be thread-safe (`Send + Sync`). The keyring may be
/// accessed from multiple threads.
pub trait StorageBackend: Send + Sync {
    /// List all entries in the storage.
    ///
    /// Returns a list of (name, data) pairs for all stored entries.
    /// The config entry (if any) should NOT be included in this list.
    fn list_entries(&self) -> StorageResult<Vec<(String, Vec<u8>)>>;

    /// Read the configuration data.
    ///
    /// Returns `Ok(None)` if no configuration exists yet.
    fn read_config(&self) -> StorageResult<Option<Vec<u8>>>;

    /// Write the configuration data.
    fn write_config(&self, data: &[u8]) -> StorageResult<()>;

    /// Write an entry to storage.
    ///
    /// If an entry with the same name exists, it should be overwritten.
    fn write_entry(&self, name: &str, data: &[u8]) -> StorageResult<()>;

    /// Remove an entry from storage.
    ///
    /// Returns `Ok(())` even if the entry doesn't exist (idempotent).
    fn remove_entry(&self, name: &str) -> StorageResult<()>;

    /// Check if an entry exists.
    fn entry_exists(&self, name: &str) -> StorageResult<bool> {
        Ok(self.list_entries()?.iter().any(|(n, _)| n == name))
    }

    /// Read a specific entry by name.
    fn read_entry(&self, name: &str) -> StorageResult<Option<Vec<u8>>> {
        Ok(self
            .list_entries()?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data))
    }

    /// Clear all entries (but not config).
    fn clear_entries(&self) -> StorageResult<()> {
        for (name, _) in self.list_entries()? {
            self.remove_entry(&name)?;
        }
        Ok(())
    }
}

/// In-memory storage backend.
///
/// Useful for testing or temporary keyring operations. Data is lost when
/// the backend is dropped.
///
/// This implementation is thread-safe using interior mutability.
#[derive(Default, Clone)]
pub struct MemoryBackend {
    inner: Arc<RwLock<MemoryBackendInner>>,
}

#[derive(Default)]
struct MemoryBackendInner {
    entries: HashMap<String, Vec<u8>>,
    config: Option<Vec<u8>>,
}

impl MemoryBackend {
    /// Create a new empty in-memory backend.
    pub fn new() -> Self {
        Self::default()
    }
}

impl StorageBackend for MemoryBackend {
    fn list_entries(&self) -> StorageResult<Vec<(String, Vec<u8>)>> {
        let inner = self
            .inner
            .read()
            .map_err(|e| StorageError::Other(format!("Failed to acquire read lock: {e}")))?;
        Ok(inner
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn read_config(&self) -> StorageResult<Option<Vec<u8>>> {
        let inner = self
            .inner
            .read()
            .map_err(|e| StorageError::Other(format!("Failed to acquire read lock: {e}")))?;
        Ok(inner.config.clone())
    }

    fn write_config(&self, data: &[u8]) -> StorageResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| StorageError::Other(format!("Failed to acquire write lock: {e}")))?;
        inner.config = Some(data.to_vec());
        Ok(())
    }

    fn write_entry(&self, name: &str, data: &[u8]) -> StorageResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| StorageError::Other(format!("Failed to acquire write lock: {e}")))?;
        inner.entries.insert(name.to_string(), data.to_vec());
        Ok(())
    }

    fn remove_entry(&self, name: &str) -> StorageResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| StorageError::Other(format!("Failed to acquire write lock: {e}")))?;
        inner.entries.remove(name);
        Ok(())
    }

    fn entry_exists(&self, name: &str) -> StorageResult<bool> {
        let inner = self
            .inner
            .read()
            .map_err(|e| StorageError::Other(format!("Failed to acquire read lock: {e}")))?;
        Ok(inner.entries.contains_key(name))
    }

    fn read_entry(&self, name: &str) -> StorageResult<Option<Vec<u8>>> {
        let inner = self
            .inner
            .read()
            .map_err(|e| StorageError::Other(format!("Failed to acquire read lock: {e}")))?;
        Ok(inner.entries.get(name).cloned())
    }

    fn clear_entries(&self) -> StorageResult<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| StorageError::Other(format!("Failed to acquire write lock: {e}")))?;
        inner.entries.clear();
        Ok(())
    }
}

/// Filesystem-based storage backend.
///
/// Stores entries as individual JSON files in a directory. The config is
/// stored in a special `keyring.json` file.
///
/// # Directory Structure
///
/// ```text
/// <root>/
/// ├── keyring.json     # Configuration file
/// ├── key1.json        # Entry files
/// ├── key2.json
/// └── ...
/// ```
#[derive(Clone)]
pub struct FilesystemBackend {
    root: PathBuf,
}

use crate::keyring::CONFIG_FILE;

impl FilesystemBackend {
    /// Create a new filesystem backend at the specified directory.
    ///
    /// Creates the directory if it doesn't exist.
    pub fn new(root: impl Into<PathBuf>) -> StorageResult<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Get the root directory path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry_path(&self, name: &str) -> PathBuf {
        self.root.join(name).with_extension("json")
    }

    fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }
}

impl StorageBackend for FilesystemBackend {
    fn list_entries(&self) -> StorageResult<Vec<(String, Vec<u8>)>> {
        let mut entries = Vec::new();

        let dir_entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(entries),
            Err(e) => return Err(e.into()),
        };

        for entry in dir_entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Failed to read directory entry: {e}");
                    continue;
                }
            };

            let path = entry.path();

            // Skip non-JSON files and the config file
            if !path.is_file() {
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            if path.file_name().is_some_and(|name| name == CONFIG_FILE) {
                continue;
            }

            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            match fs::read(&path) {
                Ok(data) => entries.push((name, data)),
                Err(e) => {
                    tracing::warn!("Failed to read entry {:?}: {e}", path);
                }
            }
        }

        Ok(entries)
    }

    fn read_config(&self) -> StorageResult<Option<Vec<u8>>> {
        let path = self.config_path();
        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn write_config(&self, data: &[u8]) -> StorageResult<()> {
        fs::write(self.config_path(), data)?;
        Ok(())
    }

    fn write_entry(&self, name: &str, data: &[u8]) -> StorageResult<()> {
        fs::write(self.entry_path(name), data)?;
        Ok(())
    }

    fn remove_entry(&self, name: &str) -> StorageResult<()> {
        let path = self.entry_path(name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn entry_exists(&self, name: &str) -> StorageResult<bool> {
        Ok(self.entry_path(name).exists())
    }

    fn read_entry(&self, name: &str) -> StorageResult<Option<Vec<u8>>> {
        let path = self.entry_path(name);
        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn clear_entries(&self) -> StorageResult<()> {
        let dir_entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|ext| ext == "json")
                && path.file_name().is_none_or(|name| name != CONFIG_FILE)
            {
                fs::remove_file(&path)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_backend_operations<B: StorageBackend>(backend: B) {
        // Initially empty
        assert!(backend.list_entries().unwrap().is_empty());
        assert!(backend.read_config().unwrap().is_none());

        // Write an entry
        backend.write_entry("test1", b"data1").unwrap();
        assert!(backend.entry_exists("test1").unwrap());
        assert_eq!(
            backend.read_entry("test1").unwrap(),
            Some(b"data1".to_vec())
        );

        // Write another entry
        backend.write_entry("test2", b"data2").unwrap();
        let entries = backend.list_entries().unwrap();
        assert_eq!(entries.len(), 2);

        // Write config
        backend.write_config(b"config_data").unwrap();
        assert_eq!(
            backend.read_config().unwrap(),
            Some(b"config_data".to_vec())
        );

        // Overwrite entry
        backend.write_entry("test1", b"new_data1").unwrap();
        assert_eq!(
            backend.read_entry("test1").unwrap(),
            Some(b"new_data1".to_vec())
        );

        // Remove entry
        backend.remove_entry("test1").unwrap();
        assert!(!backend.entry_exists("test1").unwrap());
        assert!(backend.read_entry("test1").unwrap().is_none());

        // Remove non-existent entry (should be ok)
        backend.remove_entry("nonexistent").unwrap();

        // Clear entries
        backend.clear_entries().unwrap();
        assert!(backend.list_entries().unwrap().is_empty());

        // Config should still exist
        assert!(backend.read_config().unwrap().is_some());
    }

    #[test]
    fn test_memory_backend() {
        let backend = MemoryBackend::new();
        test_backend_operations(backend);
    }

    #[test]
    fn test_filesystem_backend() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backend = FilesystemBackend::new(temp_dir.path()).unwrap();
        test_backend_operations(backend);
    }

    #[test]
    fn test_memory_backend_clone() {
        let backend1 = MemoryBackend::new();
        backend1.write_entry("test", b"data").unwrap();

        let backend2 = backend1.clone();
        assert_eq!(backend2.read_entry("test").unwrap(), Some(b"data".to_vec()));

        // Changes through one clone are visible to the other
        backend2.write_entry("test2", b"data2").unwrap();
        assert!(backend1.entry_exists("test2").unwrap());
    }
}
