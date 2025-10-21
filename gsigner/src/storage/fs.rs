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

//! Filesystem-based key storage implementation.

use crate::{
    SignerError,
    error::Result,
    traits::{KeyStorage, SignatureScheme},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, marker::PhantomData, path::PathBuf};
use tempfile::TempDir;

/// Filesystem-based key storage.
///
/// Keys are stored as individual files in a directory, with the filename
/// derived from the public key representation.
#[derive(Debug)]
pub struct FSKeyStorage<S: SignatureScheme> {
    /// Path to the storage directory.
    pub path: PathBuf,
    /// Temporary directory (if using temporary storage).
    _tmp_dir: Option<TempDir>,
    _phantom: PhantomData<S>,
}

impl<S: SignatureScheme> FSKeyStorage<S> {
    /// Create filesystem storage at the specified path.
    pub fn from_path(path: PathBuf) -> Self {
        Self {
            path,
            _tmp_dir: None,
            _phantom: PhantomData,
        }
    }

    /// Create temporary filesystem storage.
    pub fn tmp() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let path = temp_dir.path().to_path_buf();

        Self {
            path,
            _tmp_dir: Some(temp_dir),
            _phantom: PhantomData,
        }
    }

    /// Get the filename for a public key.
    ///
    /// This method should be overridden by scheme-specific implementations
    /// to provide appropriate key naming.
    fn key_filename(&self, public_key: &S::PublicKey) -> String {
        format!("{public_key:?}").replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
    }
}

impl<S> KeyStorage<S> for FSKeyStorage<S>
where
    S: SignatureScheme,
    S::PrivateKey: Serialize + DeserializeOwned,
    S::PublicKey: Serialize + DeserializeOwned,
{
    fn empty() -> Self {
        Self::tmp()
    }

    fn add_key(&mut self, private_key: S::PrivateKey) -> Result<S::PublicKey> {
        let public_key = S::public_key(&private_key);
        let key_file = self.path.join(self.key_filename(&public_key));

        let serialized = serde_json::to_vec_pretty(&private_key)?;
        fs::write(key_file, serialized)?;

        Ok(public_key)
    }

    fn get_private_key(&self, public_key: S::PublicKey) -> Result<S::PrivateKey> {
        let key_path = self.path.join(self.key_filename(&public_key));

        if !key_path.exists() {
            return Err(SignerError::KeyNotFound(format!("{public_key:?}")));
        }

        let bytes = fs::read(key_path)?;
        let private_key = serde_json::from_slice(&bytes)?;

        Ok(private_key)
    }

    fn has_key(&self, public_key: S::PublicKey) -> Result<bool> {
        let key_path = self.path.join(self.key_filename(&public_key));
        Ok(key_path.exists())
    }

    fn list_keys(&self) -> Result<Vec<S::PublicKey>> {
        let mut keys = Vec::new();

        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let bytes = fs::read(entry.path())?;
                if let Ok(private_key) = serde_json::from_slice::<S::PrivateKey>(&bytes) {
                    keys.push(S::public_key(&private_key));
                }
            }
        }

        Ok(keys)
    }

    fn clear_keys(&mut self) -> Result<()> {
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}
