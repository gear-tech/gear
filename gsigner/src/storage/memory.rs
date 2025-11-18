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

//! In-memory key storage implementation.

use crate::{
    SignerError,
    error::Result,
    traits::{KeyStorage, SignatureScheme},
};
use alloc::{collections::BTreeMap, format, vec::Vec};

/// In-memory key storage using a BTreeMap.
///
/// This storage is ephemeral and will be lost when the process terminates.
/// Useful for testing and temporary key operations.
#[derive(Debug, Clone, Default)]
pub struct MemoryKeyStorage<S: SignatureScheme> {
    keys: BTreeMap<S::PublicKey, S::PrivateKey>,
}

impl<S: SignatureScheme> MemoryKeyStorage<S> {
    /// Create a new empty memory storage.
    pub fn new() -> Self {
        Self {
            keys: BTreeMap::new(),
        }
    }
}

impl<S: SignatureScheme> KeyStorage<S> for MemoryKeyStorage<S> {
    fn empty() -> Self {
        Self::new()
    }

    #[allow(clippy::clone_on_copy)]
    fn add_key(&mut self, private_key: S::PrivateKey) -> Result<S::PublicKey> {
        let public_key = S::public_key(&private_key);
        self.keys.insert(public_key.clone(), private_key);
        Ok(public_key)
    }

    fn get_private_key(&self, public_key: S::PublicKey) -> Result<S::PrivateKey> {
        self.keys
            .get(&public_key)
            .cloned()
            .ok_or_else(|| SignerError::KeyNotFound(format!("{public_key:?}")))
    }

    fn has_key(&self, public_key: S::PublicKey) -> Result<bool> {
        Ok(self.keys.contains_key(&public_key))
    }

    fn list_keys(&self) -> Result<Vec<S::PublicKey>> {
        Ok(self.keys.keys().cloned().collect())
    }

    fn clear_keys(&mut self) -> Result<()> {
        self.keys.clear();
        Ok(())
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[cfg(feature = "secp256k1")]
    #[test]
    fn test_memory_storage_secp256k1() {
        use crate::schemes::secp256k1::Secp256k1;

        let mut storage = MemoryKeyStorage::<Secp256k1>::new();
        let (private_key, expected_public_key) = Secp256k1::generate_keypair();

        let public_key = storage.add_key(private_key.clone()).unwrap();
        assert_eq!(public_key, expected_public_key);

        assert!(storage.has_key(public_key).unwrap());

        let retrieved_key = storage.get_private_key(public_key).unwrap();
        assert_eq!(retrieved_key, private_key);

        let keys = storage.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&public_key));

        storage.clear_keys().unwrap();
        assert_eq!(storage.list_keys().unwrap().len(), 0);
    }
}
