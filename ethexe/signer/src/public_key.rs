// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Public key type.

use crate::{utils, Address, PrivateKey};
use anyhow::{Error, Result};
use secp256k1::PublicKey as Secp256k1PublicKey;
use std::{fmt, str::FromStr};

/// Public key.
///
/// Basically, public key is a point on the elliptic curve, which should have
/// two coordinates - `x` and `y`, both 256 bits unsigned integers. But it's possible
/// to store only `x` coordinate, as `y` can be calculated.
///
/// As the secp256k1 elliptic curve is symmetric, the y can be either positive or
/// negative. To stress the exact position of the `y` the prefix byte is used, so
/// the public key becomes 33 bytes, not 32.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PublicKey(pub [u8; 33]);

impl PublicKey {
    /// Create public key from the private key.
    ///
    /// Only `ethexe-signer` types are used.
    pub fn from_private(private_key: PrivateKey) -> Self {
        let secret_key = private_key.into();
        let public_key = Secp256k1PublicKey::from_secret_key_global(&secret_key);

        public_key.into()
    }

    pub fn try_from_slice(slice: &[u8]) -> Result<Self> {
        let bytes = <[u8; 33]>::try_from(slice)?;

        Ok(Self::from_bytes(bytes))
    }

    /// Create public key from compressed public key bytes.
    pub fn from_bytes(bytes: [u8; 33]) -> Self {
        Self(bytes)
    }

    /// Public key hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Convert public key to ethereum address.
    pub fn to_address(&self) -> Address {
        (*self).into()
    }
}

impl From<PrivateKey> for PublicKey {
    fn from(key: PrivateKey) -> Self {
        Self::from_private(key)
    }
}

impl From<Secp256k1PublicKey> for PublicKey {
    fn from(key: Secp256k1PublicKey) -> Self {
        Self(key.serialize())
    }
}

impl From<PublicKey> for Secp256k1PublicKey {
    fn from(key: PublicKey) -> Self {
        Secp256k1PublicKey::from_byte_array_compressed(&key.0).expect("invalid public key")
    }
}

impl FromStr for PublicKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self(utils::decode_to_array(s)?))
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Error;

    fn try_from(data: &[u8]) -> Result<Self> {
        Self::try_from_slice(data)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}
