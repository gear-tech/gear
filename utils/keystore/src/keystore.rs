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
use serde::{Deserialize, Serialize};

/// JSON keystore for storing substrate key pair.
///
/// This json format is synced with the [polkadot-js implementation](https://github.com/polkadot-js/common/blob/6971012f4af62f453ba25d83d0ebbfd12eaf5709/packages/util-crypto/src/json/encryptFormat.ts#L9)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keystore {
    /// The encoded keypair in base58.
    pub encoded: String,
    /// Encoding format.
    pub encoding: Encoding,
    /// The address of the keypair.
    pub address: Option<String>,
    /// The meta data of the keypair.
    pub meta: Option<Meta>,
}

/// Encoding format for the keypair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Encoding {
    /// The content of the keystore.
    ///
    /// - The first element is the standard.
    /// - The second element is the key algorithm.
    ///
    /// The should be fixed with ["pkcs8", "sr25519"] for now.
    pub content: [String; 2],

    /// The type of the keystore.
    ///
    /// - The first element is the key deriven function of the keystore.
    /// - The second element is the encryption cipher of the keystore.
    #[serde(rename = "type")]
    pub ty: [String; 2],

    /// The version of the keystore.
    pub version: u8,
}

/// The metadata of the key pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    /// The genesis hash of the chain.
    pub genesis_hash: String,
    /// The name of the key pair.
    pub name: String,
    /// The timestamp when the key pair is created.
    pub when_created: u64,
}
