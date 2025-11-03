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

use super::{PrivateKey, PublicKey};
use crate::{
    address::SubstrateAddress,
    keyring::{
        Keyring as GenericKeyring,
        simple::{SimpleKeyCodec, SubstrateKeystore},
    },
};
use anyhow::{Result, anyhow};
use hex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ed25519Codec;

impl SimpleKeyCodec for Ed25519Codec {
    type Pair = sp_core::ed25519::Pair;
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Address = SubstrateAddress;

    fn derive_public(private_key: &Self::PrivateKey) -> Self::PublicKey {
        private_key.public_key()
    }

    fn derive_address(public_key: &Self::PublicKey) -> Result<Self::Address> {
        public_key
            .to_address()
            .map_err(|err| anyhow!("Failed to derive address: {err}"))
    }

    fn encode_private(private_key: &Self::PrivateKey) -> Result<String> {
        Ok(hex::encode(private_key.to_bytes()))
    }

    fn decode_private(encoded: &str) -> Result<Self::PrivateKey> {
        let bytes = hex::decode(encoded)?;
        if bytes.len() != 32 {
            return Err(anyhow!("Invalid ed25519 seed length"));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Ok(PrivateKey::from_seed(seed)?)
    }

    fn encode_public(public_key: &Self::PublicKey) -> Result<String> {
        Ok(hex::encode(public_key.to_bytes()))
    }

    fn decode_public(encoded: &str) -> Result<Self::PublicKey> {
        let bytes = hex::decode(encoded)?;
        if bytes.len() != 32 {
            return Err(anyhow!("Invalid ed25519 public key length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(PublicKey::from_bytes(arr))
    }

    fn encode_address(address: &Self::Address) -> Result<String> {
        Ok(address.as_ss58().to_string())
    }

    fn decode_address(encoded: &str) -> Result<Self::Address> {
        SubstrateAddress::from_ss58(encoded).map_err(|err| anyhow!("Invalid SS58 address: {err}"))
    }
}

/// JSON keystore representation for ed25519 keys.
pub type Keystore = SubstrateKeystore<Ed25519Codec>;

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
