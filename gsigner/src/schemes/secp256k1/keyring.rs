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

//! Keyring manager for secp256k1 keys.

use super::{Address, PrivateKey, PublicKey};
use crate::keyring::{Keyring as GenericKeyring, KeystoreEntry};
use anyhow::{Result, anyhow};
use core::str::FromStr;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JSON keystore representation for secp256k1 keys.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Keystore {
    /// Human readable key name.
    pub name: String,
    /// Hex-encoded compressed public key.
    pub public_key: String,
    /// Hex-encoded Ethereum address.
    pub address: String,
    /// Hex-encoded private key (prefixed with 0x).
    pub private_key: String,
    #[serde(default)]
    pub meta: Meta,
}

/// Metadata for secp256k1 keystores.
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
    pub fn from_private_key(name: &str, private_key: PrivateKey) -> Self {
        let public_key = PublicKey::from(private_key);
        let address = Address::from(public_key);

        Self {
            name: name.to_string(),
            public_key: format!("0x{}", public_key.to_hex()),
            address: format!("0x{}", address.to_hex()),
            private_key: format!("{private_key}"),
            meta: Meta::default(),
        }
    }

    /// Decode the stored private key.
    pub fn private_key(&self) -> Result<PrivateKey> {
        PrivateKey::from_str(&self.private_key).map_err(|err| anyhow!("Invalid private key: {err}"))
    }

    /// Decode the stored public key.
    pub fn public_key(&self) -> Result<PublicKey> {
        PublicKey::from_str(&self.public_key).map_err(|err| anyhow!("Invalid public key: {err}"))
    }

    /// Decode the stored address.
    pub fn address(&self) -> Result<Address> {
        Address::from_str(&self.address).map_err(|err| anyhow!("Invalid address: {err}"))
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

/// secp256k1 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add an existing private key to the keyring.
    pub fn add(&mut self, name: &str, private_key: PrivateKey) -> Result<Keystore> {
        let keystore = Keystore::from_private_key(name, private_key);
        self.store(name, keystore)
    }

    /// Add a private key from its hex representation.
    pub fn add_hex(&mut self, name: &str, hex: &str) -> Result<Keystore> {
        let private_key =
            PrivateKey::from_str(hex).map_err(|err| anyhow!("Invalid private key: {err}"))?;
        self.add(name, private_key)
    }

    /// Generate and store a new private key.
    pub fn create(&mut self, name: &str) -> Result<(Keystore, PrivateKey)> {
        let private_key = PrivateKey::random();
        let keystore = self.add(name, private_key)?;
        Ok((keystore, private_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyring::KeystoreEntry;

    #[test]
    fn create_and_recover_private_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut keyring = Keyring::load(temp_dir.path().to_path_buf()).unwrap();

        let (keystore, private_key) = keyring.create("alice").unwrap();

        assert_eq!(keystore.name, "alice");
        assert_eq!(keyring.list().len(), 1);
        assert_eq!(keyring.list()[0].name(), "alice");
        assert_eq!(keystore.private_key().unwrap(), private_key);
    }
}
