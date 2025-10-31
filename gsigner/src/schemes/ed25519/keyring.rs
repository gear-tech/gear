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

//! Keyring manager for ed25519 keys.

use super::{Ed25519, PrivateKey, PublicKey};
use crate::{
    address::SubstrateAddress,
    keyring::{Keyring as GenericKeyring, KeystoreEntry},
    traits::SignatureScheme,
};
use anyhow::{Result, anyhow};
use hex;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JSON keystore representation for ed25519 keys.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Keystore {
    /// Human readable key name.
    pub name: String,
    /// Hex-encoded public key.
    pub public_key: String,
    /// SS58-encoded address.
    pub address: String,
    /// Hex-encoded private key seed.
    pub private_key: String,
    #[serde(default)]
    pub meta: Meta,
}

/// Metadata for ed25519 keystores.
#[derive(Clone, Serialize, Deserialize)]
pub struct Meta {
    #[serde(rename = "whenCreated")]
    pub when_created: u128,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            when_created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_millis(),
        }
    }
}

impl Keystore {
    /// Create a keystore entry from a private key.
    pub fn from_private_key(name: &str, private_key: PrivateKey) -> Result<Self> {
        let public_key = Ed25519::public_key(&private_key);
        let address = public_key
            .to_address()
            .map_err(|err| anyhow!("Failed to derive address: {err}"))?;

        Ok(Self {
            name: name.to_string(),
            public_key: hex::encode(public_key.to_bytes()),
            address: address.as_ss58().to_string(),
            private_key: hex::encode(private_key.to_bytes()),
            meta: Meta::default(),
        })
    }

    /// Decode the stored private key.
    pub fn private_key(&self) -> Result<PrivateKey> {
        let bytes = hex::decode(&self.private_key)?;
        if bytes.len() != 32 {
            return Err(anyhow!("Invalid ed25519 seed length"));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Ok(PrivateKey::from_seed(seed)?)
    }

    /// Decode the stored public key.
    pub fn public_key(&self) -> Result<PublicKey> {
        let bytes = hex::decode(&self.public_key)?;
        if bytes.len() != 32 {
            return Err(anyhow!("Invalid ed25519 public key length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(PublicKey::from_bytes(arr))
    }

    /// Decode the stored address.
    pub fn address(&self) -> Result<SubstrateAddress> {
        SubstrateAddress::from_ss58(&self.address)
    }
}

impl KeystoreEntry for Keystore {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

/// ed25519 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add an existing private key to the keyring.
    pub fn add(&mut self, name: &str, private_key: PrivateKey) -> Result<Keystore> {
        let keystore = Keystore::from_private_key(name, private_key)?;
        self.store(name, keystore)
    }

    /// Add a private key from its hex-encoded seed.
    pub fn add_hex(&mut self, name: &str, hex_seed: &str) -> Result<Keystore> {
        let bytes = hex::decode(hex_seed)?;
        if bytes.len() != 32 {
            return Err(anyhow!("Invalid ed25519 seed length"));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let private_key = PrivateKey::from_seed(seed)?;
        self.add(name, private_key)
    }

    /// Generate and store a new private key.
    pub fn create(&mut self, name: &str) -> Result<(Keystore, PrivateKey)> {
        let private_key = PrivateKey::random();
        let keystore = self.add(name, private_key.clone())?;
        Ok((keystore, private_key))
    }

    /// Import a private key from a Substrate SURI.
    pub fn import_suri(
        &mut self,
        name: &str,
        suri: &str,
        password: Option<&str>,
    ) -> Result<(Keystore, PrivateKey)> {
        let private_key = PrivateKey::from_suri(suri, password)?;
        let keystore = self.add(name, private_key.clone())?;
        Ok((keystore, private_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyring::KeystoreEntry;

    #[test]
    fn create_and_restore_private_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut keyring = Keyring::load(temp_dir.path().to_path_buf()).unwrap();

        let (keystore, private_key) = keyring.create("alice").unwrap();

        assert_eq!(keyring.list().len(), 1);
        assert_eq!(keyring.list()[0].name(), "alice");
        assert_eq!(keystore.private_key().unwrap(), private_key);
    }
}
