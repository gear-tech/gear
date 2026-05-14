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
    keyring::{KeyCodec, Keyring as GenericKeyring, SubstrateKeystore},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ed25519Codec;

impl KeyCodec for Ed25519Codec {
    type Pair = sp_core::ed25519::Pair;
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Address = SubstrateAddress;

    const KEY_TYPE: &'static str = "ed25519";

    fn derive_public(private_key: &Self::PrivateKey) -> Self::PublicKey {
        crate::keyring::codec_defaults::derive_public(private_key)
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
        PrivateKey::from_seed(seed).map_err(|e| anyhow!("Invalid seed: {e}"))
    }

    fn encode_public(public_key: &Self::PublicKey) -> Result<String> {
        crate::keyring::codec_defaults::encode_public(public_key)
    }

    fn decode_public(encoded: &str) -> Result<Self::PublicKey> {
        crate::keyring::codec_defaults::decode_public(encoded)
    }

    fn encode_address(address: &Self::Address) -> Result<String> {
        crate::keyring::codec_defaults::encode_ss58_address(address)
    }

    fn decode_address(encoded: &str) -> Result<Self::Address> {
        crate::keyring::codec_defaults::decode_ss58_address(encoded)
    }

    fn random_private() -> Result<Self::PrivateKey> {
        crate::keyring::codec_defaults::random_private()
    }

    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        PrivateKey::from_suri(suri, password).map_err(|e| anyhow!("Invalid SURI: {e}"))
    }
}

/// JSON keystore representation for ed25519 keys.
pub type Keystore = SubstrateKeystore<Ed25519Codec>;

/// ed25519 keyring backed by the generic [`GenericKeyring`].
pub type Keyring = GenericKeyring<Keystore>;

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
