// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::{ss58, Keystore};
use anyhow::Result;
use schnorrkel::Keypair;
use std::{fs, path::PathBuf};

/// Gear keyring.
pub struct Keyring {
    /// Path to the store.
    pub store: PathBuf,
    /// A set of keystore instances.
    ring: Vec<Keystore>,
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

                serde_json::from_slice(&content)
                    .map_err(|err| {
                        tracing::warn!("Failed to load keystore at {path:?}: {err}");
                        err
                    })
                    .ok()
            })
            .collect::<Vec<_>>();

        Ok(Self { ring, store })
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

            while !ss58::encode(&keypair.public.to_bytes()).starts_with(vanity) {
                keypair = Keypair::generate();
            }

            keypair
        } else {
            Keypair::generate()
        };

        let mut keystore = Keystore::encrypt(keypair.clone(), passphrase.map(|p| p.as_bytes()))?;
        keystore.meta.name = name.into();

        fs::write(
            self.store.join(&keystore.meta.name).with_extension("json"),
            serde_json::to_vec_pretty(&keystore)?,
        )?;

        self.ring.push(keystore.clone());
        Ok((keystore, keypair))
    }

    /// List all keystores.
    pub fn list(&self) -> &[Keystore] {
        self.ring.as_ref()
    }
}
