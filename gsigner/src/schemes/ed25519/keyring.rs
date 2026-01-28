// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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
        key_codec::{
            KeyCodec, KeyringCodecExt, SubstrateKeystore,
            keyring_ops::{
                add_hex as keyring_add_hex, add_private as keyring_add, create as keyring_create,
                import_suri as keyring_import_suri,
            },
        },
    },
};
use anyhow::{Result, anyhow};
use hex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ed25519Codec;

impl KeyCodec for Ed25519Codec {
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

impl KeyringCodecExt for Ed25519Codec {
    fn random_private() -> Result<Self::PrivateKey> {
        Ok(PrivateKey::random())
    }

    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        Ok(PrivateKey::from_suri(suri, password)?)
    }
}

/// JSON keystore representation for ed25519 keys.
pub type Keystore = SubstrateKeystore<Ed25519Codec>;

/// ed25519 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add an existing private key to the keyring.
    pub fn add(
        &mut self,
        name: &str,
        private_key: PrivateKey,
        password: Option<&str>,
    ) -> Result<Keystore> {
        keyring_add::<Ed25519Codec>(self, name, private_key, password)
    }

    /// Add a private key from its hex-encoded seed.
    pub fn add_hex(
        &mut self,
        name: &str,
        hex_seed: &str,
        password: Option<&str>,
    ) -> Result<Keystore> {
        keyring_add_hex::<Ed25519Codec>(self, name, hex_seed, password)
    }

    /// Generate and store a new private key.
    pub fn create(&mut self, name: &str, password: Option<&str>) -> Result<(Keystore, PrivateKey)> {
        keyring_create::<Ed25519Codec>(self, name, password)
    }

    /// Import a private key from a Substrate SURI.
    pub fn import_suri(
        &mut self,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<(Keystore, PrivateKey)> {
        keyring_import_suri::<Ed25519Codec>(self, name, suri, suri_password, encryption_password)
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

        let (keystore, private_key) = keyring.create("alice", None).unwrap();

        assert_eq!(keyring.list().len(), 1);
        assert_eq!(keyring.list()[0].name(), "alice");
        assert_eq!(keystore.private_key().unwrap(), private_key);
    }
}
