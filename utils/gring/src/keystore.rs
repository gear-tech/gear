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

use crate::{Keypair, KeypairInfo, Scrypt, ss58};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::RngCore;
// use schnorrkel::Keypair;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JSON keystore for storing sr25519 key pair.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Keystore {
    /// The encoded keypair in base64.
    pub encoded: String,
    /// Encoding format.
    #[serde(default)]
    pub encoding: Encoding,
    /// The address of the keypair.
    pub address: String,
    /// The meta data of the keypair.
    #[serde(default)]
    pub meta: Meta,
}

impl Keystore {
    /// The length of nonce.
    const NONCE_LENGTH: usize = 24;

    /// Encrypt the provided keypair with the given password.
    pub fn encrypt(keypair: Keypair, passphrase: Option<&[u8]>) -> Result<Self> {
        let info = KeypairInfo::from(keypair);
        if let Some(passphrase) = passphrase {
            Self::encrypt_scrypt(info, passphrase)
        } else {
            Self::encrypt_none(info)
        }
    }

    /// Encrypt keypair info with scrypt.
    pub fn encrypt_scrypt(info: KeypairInfo, passphrase: &[u8]) -> Result<Self> {
        let mut encoded = Vec::new();

        // 1. Get passwd from scrypt
        let scrypt = Scrypt::default();
        let passwd = scrypt.passwd(passphrase)?;
        encoded.extend_from_slice(&scrypt.encode());

        // 2. Generate random nonce
        let mut nonce = [0; Self::NONCE_LENGTH];
        rand::thread_rng().fill_bytes(&mut nonce);
        encoded.extend_from_slice(&nonce);

        // 3. Pack secret box
        let encrypted = nacl::secret_box::pack(&info.encode(), &nonce, &passwd[..32])
            .map_err(|e| anyhow!("{e:?}"))?;
        encoded.extend_from_slice(&encrypted);

        Ok(Self {
            encoded: STANDARD.encode(&encoded),
            address: ss58::encode(&info.public)?,
            encoding: Encoding::scrypt(),
            ..Default::default()
        })
    }

    /// Encrypt keypair without encryption.
    pub fn encrypt_none(info: KeypairInfo) -> Result<Self> {
        Ok(Self {
            encoded: STANDARD.encode(info.encode()),
            address: ss58::encode(&info.public)?,
            ..Default::default()
        })
    }

    /// Decrypt keypair from encrypted data.
    pub fn decrypt(&self, passphrase: Option<&[u8]>) -> Result<Keypair> {
        if let Some(passphrase) = passphrase {
            if !self.encoding.is_scrypt() {
                return Err(anyhow!(
                    "unsupported key deriven function {}.",
                    self.encoding.ty[0]
                ));
            }

            self.decrypt_scrypt(passphrase)
        } else {
            if self.encoding.is_xsalsa20_poly1305() {
                return Err(anyhow!("password required to decode encrypted data."));
            }

            self.decrypt_none()
        }
    }

    /// Decrypt keypair from encrypted data with scrypt.
    pub fn decrypt_scrypt(&self, passphrase: &[u8]) -> Result<Keypair> {
        let decoded = self.decoded()?;

        // 1. Get passwd from scrypt
        let mut encoded_scrypt = [0; Scrypt::ENCODED_LENGTH];
        encoded_scrypt.copy_from_slice(&decoded[..Scrypt::ENCODED_LENGTH]);
        let passwd = Scrypt::decode(encoded_scrypt).passwd(passphrase)?;

        // 2. Decrypt the secret key with xsalsa20-poly1305
        let encrypted = &decoded[Scrypt::ENCODED_LENGTH..];
        let secret = nacl::secret_box::open(
            &encrypted[Self::NONCE_LENGTH..],
            &encrypted[..Self::NONCE_LENGTH],
            &passwd[..32],
        )
        .map_err(|e| anyhow!("{e:?}"))?;

        // 3. Decode the secret key to keypair
        KeypairInfo::decode(&secret[..KeypairInfo::ENCODED_LENGTH])?.into_keypair()
    }

    /// Decrypt keypair from data without encryption.
    pub fn decrypt_none(&self) -> Result<Keypair> {
        KeypairInfo::decode(&self.decoded()?)?.into_keypair()
    }

    /// Returns self with the given name in meta.
    pub fn with_name(mut self, name: &str) -> Self {
        self.meta.name = name.to_owned();
        self
    }

    /// Decode the encoded keypair info with base64.
    fn decoded(&self) -> Result<Vec<u8>> {
        STANDARD.decode(&self.encoded).map_err(Into::into)
    }
}

/// Encoding format for the keypair.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Encoding {
    /// The content of the keystore.
    ///
    /// - The first element is the standard.
    /// - The second element is the key algorithm.
    pub content: (String, String),

    /// The type of the keystore.
    ///
    /// - The first element is the key deriven function of the keystore.
    ///   - if the first element is `none`, there will be no cipher following.
    /// - The second element is the encryption cipher of the keystore.
    #[serde(rename = "type")]
    pub ty: Vec<String>,

    /// The version of the keystore.
    pub version: String,
}

impl Encoding {
    /// None encoding format.
    pub fn none() -> Self {
        Self {
            content: ("pkcs8".into(), "sr25519".into()),
            ty: vec!["none".into()],
            version: "3".to_string(),
        }
    }

    /// Recommend encoding format.
    pub fn scrypt() -> Self {
        Self {
            content: ("pkcs8".into(), "sr25519".into()),
            ty: vec!["scrypt".into(), "xsalsa20-poly1305".into()],
            ..Default::default()
        }
    }

    /// Check if is encoding with scrypt.
    pub fn is_scrypt(&self) -> bool {
        self.ty.first() == Some(&"scrypt".into())
    }

    /// Check if the cipher is xsalsa20-poly1305.
    pub fn is_xsalsa20_poly1305(&self) -> bool {
        self.ty.get(1) == Some(&"xsalsa20-poly1305".into())
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::none()
    }
}

/// The metadata of the key pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    /// The name of the key pair.
    pub name: String,

    /// The timestamp when the key pair is created in milliseconds.
    #[serde(rename = "whenCreated")]
    pub when_created: u128,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            name: "".into(),
            when_created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_millis(),
        }
    }
}
