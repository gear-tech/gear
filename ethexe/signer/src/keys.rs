// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::Address;
use anyhow::{Error, Result};
use derive_more::{Debug, Display, From, Into};
use k256::ecdsa::{SigningKey, VerifyingKey};
use parity_scale_codec::{Decode, Encode};
use std::str::FromStr;

/// Private key.
///
/// Private key type used for elliptic curves maths for secp256k1 standard
/// is a 256 bits unsigned integer, which the type stores as a 32 bytes array.
#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, Hash, Debug, Display, From, Into)]
#[debug("0x{}", hex::encode(_0))]
#[display("0x{}", hex::encode(_0))]
pub struct PrivateKey([u8; 32]);

impl From<SigningKey> for PrivateKey {
    fn from(key: SigningKey) -> Self {
        let bytes = key.to_bytes();
        Self(bytes.into())
    }
}

impl From<PrivateKey> for SigningKey {
    fn from(key: PrivateKey) -> Self {
        SigningKey::from_bytes((&key.0).into()).expect("invalid private key")
    }
}

impl FromStr for PrivateKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(crate::decode_to_array(s)?))
    }
}

/// Public key.
///
/// Basically, public key is a point on the elliptic curve, which should have
/// two coordinates - `x` and `y`, both 256 bits unsigned integers. But it's possible
/// to store only `x` coordinate, as `y` can be calculated.
///
/// As the secp256k1 elliptic curve is symmetric, the y can be either positive or
/// negative. To stress the exact position of the `y` the prefix byte is used, so
/// the public key becomes 33 bytes, not 32.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Debug, Display, From, Into)]
#[debug("0x{}", self.to_hex())]
#[display("0x{}", self.to_hex())]
pub struct PublicKey(pub [u8; 33]);

impl PublicKey {
    /// Create public key from the private key.
    ///
    /// Only `ethexe-signer` types are used.
    pub fn from_private(private_key: PrivateKey) -> Self {
        let signing_key: SigningKey = private_key.into();
        let verifying_key = VerifyingKey::from(&signing_key);

        verifying_key.into()
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

    /// Convert public key to uncompressed public key bytes.
    pub fn to_uncompressed(&self) -> [u8; 64] {
        VerifyingKey::from(*self).to_encoded_point(false).as_bytes()[1..]
            .try_into()
            .expect("uncompressed key expected")
    }
}

impl From<PrivateKey> for PublicKey {
    fn from(key: PrivateKey) -> Self {
        Self::from_private(key)
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(key: VerifyingKey) -> Self {
        Self(
            key.to_encoded_point(true)
                .as_bytes()
                .try_into()
                .expect("compressed key expected"),
        )
    }
}

impl From<PublicKey> for VerifyingKey {
    fn from(key: PublicKey) -> Self {
        VerifyingKey::from_sec1_bytes(key.0.as_slice()).expect("invalid public key")
    }
}

impl FromStr for PublicKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self(crate::decode_to_array(s)?))
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Error;

    fn try_from(data: &[u8]) -> Result<Self> {
        Self::try_from_slice(data)
    }
}
