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

//! Keyring manager for secp256k1 keys.

use super::{Address, PrivateKey, PublicKey};
use crate::{
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
    substrate::pair_from_suri,
};
use anyhow::{Result, anyhow};
use core::str::FromStr;
use hex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Secp256k1Codec;

impl KeyCodec for Secp256k1Codec {
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
        // Use raw seed bytes to avoid leaking redacted Display output.
        Ok(hex::encode(private_key.seed().as_ref()))
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

impl KeyringCodecExt for Secp256k1Codec {
    fn random_private() -> Result<Self::PrivateKey> {
        Ok(PrivateKey::random())
    }

    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        let pair = pair_from_suri::<sp_core::ecdsa::Pair>(suri, password)?;
        Ok(pair.into())
    }
}

/// JSON keystore representation for secp256k1 keys.
pub type Keystore = SubstrateKeystore<Secp256k1Codec>;

/// secp256k1 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

impl Keyring {
    /// Add an existing private key to the keyring.
    pub fn add(
        &mut self,
        name: &str,
        private_key: PrivateKey,
        password: Option<&str>,
    ) -> Result<Keystore> {
        keyring_add::<Secp256k1Codec>(self, name, private_key, password)
    }

    /// Add a private key from its hex representation.
    pub fn add_hex(&mut self, name: &str, hex: &str, password: Option<&str>) -> Result<Keystore> {
        keyring_add_hex::<Secp256k1Codec>(self, name, hex, password)
    }

    /// Generate and store a new private key.
    pub fn create(&mut self, name: &str, password: Option<&str>) -> Result<(Keystore, PrivateKey)> {
        keyring_create::<Secp256k1Codec>(self, name, password)
    }

    /// Import a key from a Substrate-style SURI (mnemonic, dev URI, derivation path).
    pub fn import_suri(
        &mut self,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<(Keystore, PrivateKey)> {
        keyring_import_suri::<Secp256k1Codec>(self, name, suri, suri_password, encryption_password)
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

        let (keystore, private_key) = keyring.create("alice", None).unwrap();

        assert_eq!(keystore.name, "alice");
        assert_eq!(keyring.list().len(), 1);
        assert_eq!(keyring.list()[0].name(), "alice");
        assert_eq!(keystore.private_key().unwrap(), private_key);
    }
}
