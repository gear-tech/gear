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

use super::{Keystore, PrivateKey};
use crate::keyring::{Keyring as GenericKeyring, KeystoreEntry};
use anyhow::Result;
use std::path::PathBuf;

/// sr25519 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add a keypair to the keyring.
    pub fn add(
        &mut self,
        name: &str,
        private_key: PrivateKey,
        passphrase: Option<&[u8]>,
    ) -> Result<Keystore> {
        let keypair = private_key.keypair();
        let keystore = Keystore::encrypt(keypair, passphrase)?.with_name(name);
        self.store(name, keystore)
    }

    /// Create a new key in the keyring.
    pub fn create(
        &mut self,
        name: &str,
        passphrase: Option<&[u8]>,
    ) -> Result<(Keystore, PrivateKey)> {
        let private_key = PrivateKey::random();
        let keystore = self.add(name, private_key.clone(), passphrase)?;
        Ok((keystore, private_key))
    }

    /// Create a vanity key with the specified SS58 prefix.
    pub fn create_vanity(
        &mut self,
        name: &str,
        prefix: &str,
        passphrase: Option<&[u8]>,
    ) -> Result<(Keystore, PrivateKey)> {
        let private_key = loop {
            let candidate = PrivateKey::random();
            let address = candidate.public_key().to_address()?;

            if address.as_ss58().starts_with(prefix) {
                break candidate;
            }
        };

        let keystore = self.add(name, private_key.clone(), passphrase)?;
        Ok((keystore, private_key))
    }

    /// Import a polkadot-js compatible keystore file.
    pub fn import_polkadot(&mut self, path: PathBuf) -> Result<Keystore> {
        self.import(path)
    }
}

impl KeystoreEntry for Keystore {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn set_name(&mut self, name: &str) {
        self.meta.name = name.to_string();
    }
}
