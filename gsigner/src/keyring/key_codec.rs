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

//! Generic keystore helpers shared by substrate-style schemes.

use crate::{
    keyring::{KeystoreEntry, encryption},
    substrate::{HasKeyTypeId, pair_key_type_string},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sp_core::crypto::Pair as PairTrait;
use std::{
    marker::PhantomData,
    time::{SystemTime, UNIX_EPOCH},
};

/// Trait describing how to convert to and from string representations for key material.
pub trait KeyCodec {
    /// Concrete Substrate pair type.
    type Pair: PairTrait + HasKeyTypeId;
    /// Private key wrapper type.
    type PrivateKey: Clone;
    /// Public key wrapper type.
    type PublicKey: Clone;
    /// Address type exposed by the scheme.
    type Address;

    /// Derive the public key from the provided private key.
    fn derive_public(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Derive the address from the provided public key.
    fn derive_address(public_key: &Self::PublicKey) -> Result<Self::Address>;

    /// Encode private key for storage.
    fn encode_private(private_key: &Self::PrivateKey) -> Result<String>;

    /// Decode private key from storage.
    fn decode_private(encoded: &str) -> Result<Self::PrivateKey>;

    /// Encode public key for storage.
    fn encode_public(public_key: &Self::PublicKey) -> Result<String>;

    /// Decode public key from storage.
    fn decode_public(encoded: &str) -> Result<Self::PublicKey>;

    /// Encode address for storage.
    fn encode_address(address: &Self::Address) -> Result<String>;

    /// Decode address from storage.
    fn decode_address(encoded: &str) -> Result<Self::Address>;
}

/// Extension trait for keyring flows (generate/import/add) over a [`KeyCodec`].
pub trait KeyringCodecExt: KeyCodec {
    /// Generate a new private key.
    fn random_private() -> Result<Self::PrivateKey>;

    /// Import a private key from a SURI (mnemonic/derivation path).
    fn import_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey>;
}

/// Generic keystore structure compatible with the CLI keyring workflow.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct SubstrateKeystore<C: KeyCodec> {
    /// Human readable key name.
    pub name: String,
    /// Encoded public key.
    pub public_key: String,
    /// Encoded address.
    pub address: String,
    /// Encoded private key.
    pub private_key: String,
    /// Encryption metadata if the private key was stored securely.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<SecretEncryption>,
    #[serde(default)]
    pub meta: SubstrateKeystoreMeta<C>,
    #[serde(skip)]
    _marker: PhantomData<C>,
}

impl<C: KeyCodec> SubstrateKeystore<C> {
    /// Build a keystore entry from a private key.
    pub fn from_private_key(name: &str, private_key: C::PrivateKey) -> Result<Self> {
        Self::from_private_key_with_password(name, private_key, None)
    }

    /// Build a keystore entry from a private key with optional encryption.
    pub fn from_private_key_with_password(
        name: &str,
        private_key: C::PrivateKey,
        password: Option<&str>,
    ) -> Result<Self> {
        let public_key = C::derive_public(&private_key);
        let address = C::derive_address(&public_key)?;
        let encoded_private = C::encode_private(&private_key)?;
        let (secret, encryption) = if let Some(password) = password {
            let encrypted =
                encryption::encrypt_secret(encoded_private.as_bytes(), password.as_bytes())?;
            (encrypted, Some(SecretEncryption::scrypt()))
        } else {
            (encoded_private, None)
        };
        Ok(Self {
            name: name.to_string(),
            public_key: C::encode_public(&public_key)?,
            address: C::encode_address(&address)?,
            private_key: secret,
            encryption,
            meta: SubstrateKeystoreMeta::default(),
            _marker: PhantomData,
        })
    }

    /// Decode the stored private key.
    pub fn private_key(&self) -> Result<C::PrivateKey> {
        self.private_key_with_password(None)
    }

    /// Decode the stored private key using the provided password.
    pub fn private_key_with_password(&self, password: Option<&str>) -> Result<C::PrivateKey> {
        if self.encryption.is_some() {
            let password = password.ok_or_else(|| {
                anyhow!("Password required for encrypted keystore '{}'", self.name())
            })?;
            let decrypted = encryption::decrypt_secret(&self.private_key, password.as_bytes())?;
            let encoded = String::from_utf8(decrypted)
                .map_err(|_| anyhow!("Invalid encrypted private key data"))?;
            return C::decode_private(&encoded);
        }

        C::decode_private(&self.private_key)
    }

    /// Decode the stored public key.
    pub fn public_key(&self) -> Result<C::PublicKey> {
        C::decode_public(&self.public_key)
    }

    /// Decode the stored address.
    pub fn address(&self) -> Result<C::Address> {
        C::decode_address(&self.address)
    }
}

impl<C: KeyCodec> Default for SubstrateKeystore<C> {
    fn default() -> Self {
        Self {
            name: String::new(),
            public_key: String::new(),
            address: String::new(),
            private_key: String::new(),
            encryption: None,
            meta: SubstrateKeystoreMeta::default(),
            _marker: PhantomData,
        }
    }
}

/// Metadata describing how a private key is encrypted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretEncryption {
    #[serde(rename = "type")]
    pub ty: String,
}

impl SecretEncryption {
    pub fn scrypt() -> Self {
        Self {
            ty: "scrypt-xsalsa20-poly1305".into(),
        }
    }
}

/// Metadata stored alongside keystore entries.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct SubstrateKeystoreMeta<C: KeyCodec> {
    #[serde(rename = "whenCreated")]
    pub when_created: u128,
    #[serde(
        rename = "keyType",
        default = "SubstrateKeystoreMeta::<C>::default_key_type"
    )]
    pub key_type: String,
    #[serde(skip)]
    _marker: PhantomData<C>,
}

impl<C: KeyCodec> Default for SubstrateKeystoreMeta<C> {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();
        Self {
            when_created: now,
            key_type: Self::default_key_type(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> SubstrateKeystoreMeta<C> {
    fn default_key_type() -> String {
        pair_key_type_string::<C::Pair>()
    }
}

impl<C: KeyCodec> Clone for SubstrateKeystore<C> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            public_key: self.public_key.clone(),
            address: self.address.clone(),
            private_key: self.private_key.clone(),
            encryption: self.encryption.clone(),
            meta: self.meta.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> Clone for SubstrateKeystoreMeta<C> {
    fn clone(&self) -> Self {
        Self {
            when_created: self.when_created,
            key_type: self.key_type.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: KeyCodec> KeystoreEntry for SubstrateKeystore<C> {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

/// Generic helpers to wire keyring commands for any [`KeyringCodecExt`].
pub mod keyring_ops {
    use super::{KeyringCodecExt, SubstrateKeystore};
    use crate::keyring::Keyring;
    use anyhow::Result;

    pub fn add_private<C: KeyringCodecExt>(
        keyring: &mut Keyring<SubstrateKeystore<C>>,
        name: &str,
        private_key: C::PrivateKey,
        password: Option<&str>,
    ) -> Result<SubstrateKeystore<C>> {
        let keystore =
            SubstrateKeystore::from_private_key_with_password(name, private_key, password)?;
        keyring.store(name, keystore)
    }

    pub fn add_hex<C: KeyringCodecExt>(
        keyring: &mut Keyring<SubstrateKeystore<C>>,
        name: &str,
        encoded: &str,
        password: Option<&str>,
    ) -> Result<SubstrateKeystore<C>> {
        let private_key = C::decode_private(encoded)?;
        add_private(keyring, name, private_key, password)
    }

    pub fn create<C: KeyringCodecExt>(
        keyring: &mut Keyring<SubstrateKeystore<C>>,
        name: &str,
        password: Option<&str>,
    ) -> Result<(SubstrateKeystore<C>, C::PrivateKey)> {
        let private_key = C::random_private()?;
        let keystore = add_private(keyring, name, private_key.clone(), password)?;
        Ok((keystore, private_key))
    }

    pub fn import_suri<C: KeyringCodecExt>(
        keyring: &mut Keyring<SubstrateKeystore<C>>,
        name: &str,
        suri: &str,
        suri_password: Option<&str>,
        encryption_password: Option<&str>,
    ) -> Result<(SubstrateKeystore<C>, C::PrivateKey)> {
        let private_key = C::import_suri(suri, suri_password)?;
        let keystore = add_private(keyring, name, private_key.clone(), encryption_password)?;
        Ok((keystore, private_key))
    }
}
