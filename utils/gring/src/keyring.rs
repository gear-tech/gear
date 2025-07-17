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

//! Keyring implementation based on the polkadot-js keystore.

use crate::{Keystore, ss58};
use anyhow::{Result, anyhow};
use colored::Colorize;
use schnorrkel::Keypair;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const CONFIG: &str = "keyring.json";

/// Gear keyring.
#[derive(Default, Serialize, Deserialize)]
pub struct Keyring {
    /// Path to the store.
    #[serde(skip)]
    pub store: PathBuf,
    /// A set of keystore instances.
    #[serde(skip)]
    ring: Vec<Keystore>,
    /// The primary key.
    pub primary: String,
}

impl Keyring {
    /// Loads the keyring from the store.
    ///
    /// NOTE: For the store path, see [`STORE`].
    pub fn load(store: PathBuf) -> Result<Self> {
        let ring = fs::read_dir(&store)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let content = fs::read(&path).ok()?;
                if path.ends_with(CONFIG) {
                    return None;
                }

                serde_json::from_slice(&content)
                    .map_err(|err| {
                        tracing::warn!("Failed to load keystore at {path:?}: {err}");
                        err
                    })
                    .ok()
            })
            .collect::<Vec<_>>();

        let config = store.join(CONFIG);
        let mut this = if config.exists() {
            serde_json::from_slice(&fs::read(&config)?)?
        } else {
            Self::default()
        };

        this.ring = ring;
        this.store = store;

        Ok(this)
    }

    /// Update and get the primary key.
    pub fn primary(&mut self) -> Result<Keystore> {
        if self.ring.is_empty() {
            return Err(anyhow!(
                "No keys in keyring, run {} to create a new one.",
                "`gring generate <NAME> -p <PASSPHRASE>`"
                    .underline()
                    .cyan()
                    .bold()
            ));
        }

        if let Some(key) = self
            .ring
            .iter()
            .find(|k| k.meta.name == self.primary)
            .cloned()
        {
            Ok(key)
        } else {
            self.primary = self.ring[0].meta.name.clone();
            fs::write(self.store.join(CONFIG), serde_json::to_vec_pretty(&self)?)?;
            Ok(self.ring[0].clone())
        }
    }

    /// Set the primary key.
    pub fn set_primary(&mut self, name: String) -> Result<Keystore> {
        let key = self
            .ring
            .iter()
            .find(|k| k.meta.name == name)
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "Key with name {} not found, run {} to see all keys in keyring.",
                    name.underline().bold(),
                    "`gring list`".underline().cyan().bold()
                )
            })?;

        self.primary = name;
        fs::write(self.store.join(CONFIG), serde_json::to_vec_pretty(&self)?)?;
        Ok(key)
    }

    /// Add keypair to the keyring
    pub fn add(
        &mut self,
        name: &str,
        keypair: Keypair,
        passphrase: Option<&str>,
    ) -> Result<(Keystore, Keypair)> {
        let mut keystore = Keystore::encrypt(keypair.clone(), passphrase.map(|p| p.as_bytes()))?;
        keystore.meta.name = name.into();

        fs::write(
            self.store.join(&keystore.meta.name).with_extension("json"),
            serde_json::to_vec_pretty(&keystore)?,
        )?;

        self.ring.push(keystore.clone());
        Ok((keystore, keypair))
    }

    /// create a new key in keyring.
    pub fn create(
        &mut self,
        name: &str,
        vanity: Option<&str>,
        passphrase: Option<&str>,
    ) -> Result<(Keystore, Keypair)> {
        let keypair = if let Some(vanity) = vanity {
            tracing::info!("Generating vanity key with prefix {vanity}...");
            let mut keypair = Keypair::generate();

            while !ss58::encode(&keypair.public.to_bytes())?.starts_with(vanity) {
                keypair = Keypair::generate();
            }

            keypair
        } else {
            Keypair::generate()
        };

        self.add(name, keypair, passphrase)
    }

    /// List all keystores.
    pub fn list(&self) -> &[Keystore] {
        self.ring.as_ref()
    }
}
