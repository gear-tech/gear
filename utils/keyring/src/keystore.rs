// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// JSON keystore for storing sr25519 key pair.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Keystore {
    /// The encoded keypair in base58.
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
    /// Load keystore from json file.
    pub fn load_json(path: &Path) -> Result<Self> {
        serde_json::from_slice(&fs::read(path)?).map_err(|e| anyhow!("Failed to parse json: {}", e))
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
    /// - The second element is the encryption cipher of the keystore.
    #[serde(rename = "type")]
    pub ty: (String, String),

    /// The version of the keystore.
    pub version: u8,
}

impl Encoding {
    /// Check if is encoding with scrypt.
    pub fn is_scrypt(&self) -> bool {
        self.ty.0.as_str() == "scrypt"
    }

    /// Check if the cipher is xsalsa20-poly1305.
    pub fn is_xsalsa20_poly1305(&self) -> bool {
        self.ty.1.as_str() == "xsalsa20-poly1305"
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self {
            content: ("pkcs8".into(), "sr25519".into()),
            ty: ("scrypt".into(), "xsalsa20-poly1305".into()),
            version: 3,
        }
    }
}

/// The metadata of the key pair.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Meta {
    /// The genesis hash of the chain in hex.
    pub genesis_hash: String,
    /// The name of the key pair.
    pub name: String,
    /// The timestamp when the key pair is created.
    pub when_created: u64,
}
