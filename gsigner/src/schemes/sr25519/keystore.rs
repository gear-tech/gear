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

//! Polkadot-js compatible keystore format for sr25519 keys.

use crate::{
    address::{SubstrateAddress, SubstrateCryptoScheme},
    keyring::encryption,
};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use schnorrkel::{KEYPAIR_LENGTH, Keypair, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JSON keystore for storing sr25519 keypairs (polkadot-js compatible).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Keystore {
    /// The encoded keypair in base64.
    pub encoded: String,
    /// Encoding format.
    #[serde(default)]
    pub encoding: Encoding,
    /// The SS58 address of the keypair.
    pub address: String,
    /// The metadata of the keypair.
    #[serde(default)]
    pub meta: Meta,
}

impl Keystore {
    /// Encrypt keypair with password (scrypt).
    pub fn encrypt(keypair: Keypair, passphrase: Option<&[u8]>) -> Result<Self> {
        let info = KeypairInfo::from(keypair);
        if let Some(passphrase) = passphrase {
            let encoded = encryption::encrypt_secret(&info.encode(), passphrase)?;
            let address = SubstrateAddress::new(info.public, SubstrateCryptoScheme::Sr25519)?;
            Ok(Self {
                encoded,
                address: address.as_ss58().to_string(),
                encoding: Encoding::scrypt(),
                ..Default::default()
            })
        } else {
            Self::encrypt_none(info)
        }
    }

    fn encrypt_none(info: KeypairInfo) -> Result<Self> {
        let address = SubstrateAddress::new(info.public, SubstrateCryptoScheme::Sr25519)?;

        Ok(Self {
            encoded: STANDARD.encode(info.encode()),
            address: address.as_ss58().to_string(),
            ..Default::default()
        })
    }

    /// Decrypt keypair from keystore.
    pub fn decrypt(&self, passphrase: Option<&[u8]>) -> Result<Keypair> {
        if let Some(passphrase) = passphrase {
            if !self.encoding.is_scrypt() {
                return Err(anyhow!("Unsupported encryption: {:?}", self.encoding.ty[0]));
            }
            let secret = encryption::decrypt_secret(&self.encoded, passphrase)?;
            KeypairInfo::decode(&secret)?.into_keypair()
        } else {
            if self.encoding.is_xsalsa20_poly1305() {
                return Err(anyhow!("Password required for encrypted keystore"));
            }
            self.decrypt_none()
        }
    }

    fn decrypt_none(&self) -> Result<Keypair> {
        let decoded = STANDARD.decode(&self.encoded)?;
        KeypairInfo::decode(&decoded)?.into_keypair()
    }

    /// Set name in metadata.
    pub fn with_name(mut self, name: &str) -> Self {
        self.meta.name = name.to_owned();
        self
    }

    /// Get the public key from the keystore.
    pub fn public_key(&self) -> Result<[u8; 32]> {
        // Parse the SS58 address to get the public key
        let address = SubstrateAddress::from_ss58(&self.address)?;
        Ok(address.public_key)
    }
}

/// Encoding format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Encoding {
    pub content: (String, String),
    #[serde(rename = "type")]
    pub ty: Vec<String>,
    pub version: String,
}

impl Encoding {
    pub fn none() -> Self {
        Self {
            content: ("pkcs8".into(), "sr25519".into()),
            ty: vec!["none".into()],
            version: "3".to_string(),
        }
    }

    pub fn scrypt() -> Self {
        Self {
            content: ("pkcs8".into(), "sr25519".into()),
            ty: vec!["scrypt".into(), "xsalsa20-poly1305".into()],
            version: "3".to_string(),
        }
    }

    pub fn is_scrypt(&self) -> bool {
        self.ty.first() == Some(&"scrypt".into())
    }

    pub fn is_xsalsa20_poly1305(&self) -> bool {
        self.ty.get(1) == Some(&"xsalsa20-poly1305".into())
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::none()
    }
}

/// Keystore metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    #[serde(rename = "whenCreated")]
    pub when_created: u128,
    #[serde(rename = "keyType", default = "Meta::default_key_type")]
    pub key_type: String,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            name: String::new(),
            when_created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_millis(),
            key_type: Meta::default_key_type(),
        }
    }
}

impl Meta {
    fn default_key_type() -> String {
        crate::substrate::pair_key_type_string::<sp_core::sr25519::Pair>()
    }
}

/// Keypair info in PKCS8 format.
struct KeypairInfo {
    secret: [u8; SECRET_KEY_LENGTH],
    public: [u8; PUBLIC_KEY_LENGTH],
}

impl KeypairInfo {
    const ENCODED_LENGTH: usize = 117;
    const PKCS8_HEADER: [u8; 16] = [48, 83, 2, 1, 1, 48, 5, 6, 3, 43, 101, 112, 4, 34, 4, 32];
    const PKCS8_DIVIDER: [u8; 5] = [161, 35, 3, 33, 0];

    fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut encoded = [0u8; Self::ENCODED_LENGTH];
        encoded[..16].copy_from_slice(&Self::PKCS8_HEADER);
        encoded[16..80].copy_from_slice(&self.secret);
        encoded[80..85].copy_from_slice(&Self::PKCS8_DIVIDER);
        encoded[85..117].copy_from_slice(&self.public);
        encoded
    }

    fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < Self::ENCODED_LENGTH {
            return Err(anyhow!("Invalid keypair info length"));
        }

        if data[..16] != Self::PKCS8_HEADER {
            return Err(anyhow!("Invalid PKCS8 header"));
        }

        if data[80..85] != Self::PKCS8_DIVIDER {
            return Err(anyhow!("Invalid PKCS8 divider"));
        }

        let mut secret = [0u8; SECRET_KEY_LENGTH];
        let mut public = [0u8; PUBLIC_KEY_LENGTH];
        secret.copy_from_slice(&data[16..80]);
        public.copy_from_slice(&data[85..117]);

        Ok(Self { secret, public })
    }

    fn into_keypair(self) -> Result<Keypair> {
        let mut bytes = [0u8; KEYPAIR_LENGTH];
        bytes[..SECRET_KEY_LENGTH].copy_from_slice(&self.secret);
        bytes[SECRET_KEY_LENGTH..].copy_from_slice(&self.public);

        Keypair::from_half_ed25519_bytes(&bytes)
            .map_err(|e| anyhow!("Failed to create keypair: {:?}", e))
    }
}

impl From<Keypair> for KeypairInfo {
    fn from(keypair: Keypair) -> Self {
        Self {
            secret: keypair.secret.to_ed25519_bytes(),
            public: keypair.public.to_bytes(),
        }
    }
}
