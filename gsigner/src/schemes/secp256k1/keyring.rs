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
use crate::keyring::{KeyCodec, Keyring as GenericKeyring, SubstrateKeystore};
use anyhow::{Result, anyhow};
use core::str::FromStr;
use serde::{Deserialize, Serialize};
use sp_core::crypto::Pair as PairTrait;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Secp256k1Codec;

impl KeyCodec for Secp256k1Codec {
    type Pair = sp_core::ecdsa::Pair;
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Address = Address;

    const KEY_TYPE: &'static str = "ecdsa";

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

    fn random_private() -> Result<Self::PrivateKey> {
        Ok(PrivateKey::random())
    }

    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        let (pair, _) = sp_core::ecdsa::Pair::from_string_with_seed(suri, password)
            .map_err(|e| anyhow!("Invalid SURI: {e:?}"))?;
        Ok(pair.into())
    }
}

/// JSON keystore representation for secp256k1 keys.
pub type Keystore = SubstrateKeystore<Secp256k1Codec>;

/// secp256k1 keyring backed by the generic [`GenericKeyring`].
///
/// Methods like `add`, `add_hex`, `create`, and `import_suri` are provided
/// by the generic impl in `key_codec.rs`.
pub type Keyring = GenericKeyring<Keystore>;

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
