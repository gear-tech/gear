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

//! Keyring manager for sr25519 keys with polkadot-js compatibility.

use super::Keystore;
use crate::address::SubstrateAddress;
use anyhow::{Result, anyhow};
use schnorrkel::Keypair;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const CONFIG_FILE: &str = "keyring.json";

/// Keyring configuration.
#[derive(Default, Serialize, Deserialize)]
struct KeyringConfig {
    /// The primary key name.
    primary: String,
}

/// Keyring manager for sr25519 keys.
///
/// Manages a collection of encrypted keystores with a primary key concept.
/// Compatible with polkadot-js keystore format.
pub struct Keyring {
    /// Path to the keyring directory.
    store: PathBuf,
    /// Loaded keystores.
    keystores: Vec<Keystore>,
    /// Primary key name.
    primary: String,
}

impl Keyring {
    /// Load keyring from directory.
    pub fn load(store: PathBuf) -> Result<Self> {
        fs::create_dir_all(&store)?;

        let keystores = fs::read_dir(&store)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.ends_with(CONFIG_FILE) {
                    return None;
                }

                let content = fs::read(&path).ok()?;
                serde_json::from_slice::<Keystore>(&content).ok()
            })
            .collect::<Vec<_>>();

        let config_path = store.join(CONFIG_FILE);
        let primary = if config_path.exists() {
            let config: KeyringConfig = serde_json::from_slice(&fs::read(&config_path)?)?;
            config.primary
        } else {
            String::new()
        };

        Ok(Self {
            store,
            keystores,
            primary,
        })
    }

    /// Add a keypair to the keyring.
    pub fn add(
        &mut self,
        name: &str,
        keypair: Keypair,
        passphrase: Option<&[u8]>,
    ) -> Result<Keystore> {
        let keystore = Keystore::encrypt(keypair, passphrase)?.with_name(name);

        let path = self.store.join(name).with_extension("json");
        fs::write(&path, serde_json::to_vec_pretty(&keystore)?)?;

        self.keystores.push(keystore.clone());
        Ok(keystore)
    }

    /// Create a new key in the keyring.
    pub fn create(&mut self, name: &str, passphrase: Option<&[u8]>) -> Result<(Keystore, Keypair)> {
        let keypair = Keypair::generate();
        let keystore = self.add(name, keypair.clone(), passphrase)?;
        Ok((keystore, keypair))
    }

    /// Create a vanity key with the specified SS58 prefix.
    pub fn create_vanity(
        &mut self,
        name: &str,
        prefix: &str,
        passphrase: Option<&[u8]>,
    ) -> Result<(Keystore, Keypair)> {
        let keypair = loop {
            let keypair = Keypair::generate();
            let public_key = keypair.public.to_bytes();
            let address = SubstrateAddress::new(public_key)?;

            if address.as_ss58().starts_with(prefix) {
                break keypair;
            }
        };

        let keystore = self.add(name, keypair.clone(), passphrase)?;
        Ok((keystore, keypair))
    }

    /// Get the primary keystore.
    pub fn primary(&mut self) -> Result<&Keystore> {
        if self.keystores.is_empty() {
            return Err(anyhow!("No keys in keyring"));
        }

        if let Some(keystore) = self.keystores.iter().find(|k| k.meta.name == self.primary) {
            return Ok(keystore);
        }

        // Set first key as primary if none set
        self.primary = self.keystores[0].meta.name.clone();
        self.save_config()?;
        Ok(&self.keystores[0])
    }

    /// Set the primary key by name.
    pub fn set_primary(&mut self, name: &str) -> Result<&Keystore> {
        let keystore = self
            .keystores
            .iter()
            .find(|k| k.meta.name == name)
            .ok_or_else(|| anyhow!("Key '{}' not found", name))?;

        self.primary = name.to_string();
        self.save_config()?;
        Ok(keystore)
    }

    /// List all keystores.
    pub fn list(&self) -> &[Keystore] {
        &self.keystores
    }

    /// Get keystore by name.
    pub fn get(&self, name: &str) -> Option<&Keystore> {
        self.keystores.iter().find(|k| k.meta.name == name)
    }

    /// Import a keystore file.
    pub fn import(&mut self, path: PathBuf) -> Result<Keystore> {
        let content = fs::read(&path)?;
        let keystore: Keystore = serde_json::from_slice(&content)?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&keystore.meta.name)
            .to_string();

        let mut keystore = keystore;
        keystore.meta.name = name.clone();

        let dest = self.store.join(name).with_extension("json");
        fs::write(&dest, serde_json::to_vec_pretty(&keystore)?)?;

        self.keystores.push(keystore.clone());
        Ok(keystore)
    }

    /// Save keyring configuration.
    fn save_config(&self) -> Result<()> {
        let config = KeyringConfig {
            primary: self.primary.clone(),
        };
        let path = self.store.join(CONFIG_FILE);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        Ok(())
    }
}
