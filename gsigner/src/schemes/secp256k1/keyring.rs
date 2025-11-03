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
use crate::{
    keyring::{
        Keyring as GenericKeyring,
        simple::{SimpleKeyCodec, SubstrateKeystore},
    },
    substrate_utils::pair_from_suri,
};
use anyhow::{Result, anyhow};
use core::str::FromStr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Secp256k1Codec;

impl SimpleKeyCodec for Secp256k1Codec {
    type Pair = sp_core::ecdsa::Pair;
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Address = Address;

    fn derive_public(private_key: &Self::PrivateKey) -> Self::PublicKey {
        private_key.public_key()
    }

    fn derive_address(public_key: &Self::PublicKey) -> Result<Self::Address> {
        Ok(Address::from(*public_key))
    }

    fn encode_private(private_key: &Self::PrivateKey) -> Result<String> {
        Ok(private_key.to_string())
    }

    fn decode_private(encoded: &str) -> Result<Self::PrivateKey> {
        PrivateKey::from_str(encoded).map_err(|err| anyhow!("Invalid private key: {err}"))
    }

    fn encode_public(public_key: &Self::PublicKey) -> Result<String> {
        Ok(format!("0x{}", public_key.to_hex()))
    }

    fn decode_public(encoded: &str) -> Result<Self::PublicKey> {
        PublicKey::from_str(encoded).map_err(|err| anyhow!("Invalid public key: {err}"))
    }

    fn encode_address(address: &Self::Address) -> Result<String> {
        Ok(format!("0x{}", address.to_hex()))
    }

    fn decode_address(encoded: &str) -> Result<Self::Address> {
        Address::from_str(encoded).map_err(|err| anyhow!("Invalid address: {err}"))
    }
}

/// JSON keystore representation for secp256k1 keys.
pub type Keystore = SubstrateKeystore<Secp256k1Codec>;

/// secp256k1 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add an existing private key to the keyring.
    pub fn add(&mut self, name: &str, private_key: PrivateKey) -> Result<Keystore> {
        let keystore = Keystore::from_private_key(name, private_key)?;
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
        let keystore = self.add(name, private_key.clone())?;
        Ok((keystore, private_key))
    }

    /// Import a key from a Substrate-style SURI (mnemonic, dev URI, derivation path).
    pub fn import_suri(
        &mut self,
        name: &str,
        suri: &str,
        password: Option<&str>,
    ) -> Result<(Keystore, PrivateKey)> {
        let pair = pair_from_suri::<sp_core::ecdsa::Pair>(suri, password)?;
        self.import_pair(name, pair)
    }

    fn import_pair(
        &mut self,
        name: &str,
        pair: sp_core::ecdsa::Pair,
    ) -> Result<(Keystore, PrivateKey)> {
        let private_key: PrivateKey = pair.into();
        let keystore = self.add(name, private_key.clone())?;
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
