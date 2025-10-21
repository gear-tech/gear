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

//! secp256k1 key types.

use alloc::string::String;
#[cfg(feature = "serde")]
use alloc::vec::Vec;
use core::str::FromStr;
use hex::FromHexError;
use k256::ecdsa::{SigningKey, VerifyingKey};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::Address;

/// Private key.
///
/// Private key type used for elliptic curves maths for secp256k1 standard
/// is a 256 bits unsigned integer, which the type stores as a 32 bytes array.
#[derive(
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    derive_more::Debug,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
#[cfg_attr(
    feature = "codec",
    derive(parity_scale_codec::Encode, parity_scale_codec::Decode)
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        decode_hex_to_array(s).map(Self)
    }
}

#[cfg(feature = "std")]
impl PrivateKey {
    pub fn random() -> Self {
        SigningKey::random(&mut k256::elliptic_curve::rand_core::OsRng).into()
    }
}

#[cfg(not(feature = "std"))]
impl PrivateKey {
    pub fn random() -> Self {
        panic!("`PrivateKey::random` requires the `std` feature to access an RNG");
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
#[derive(
    Clone,
    Copy,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Debug,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
#[debug("0x{}", self.to_hex())]
#[display("0x{}", self.to_hex())]
pub struct PublicKey(pub [u8; 33]);

impl PublicKey {
    /// Create public key from the private key.
    pub fn from_private(private_key: PrivateKey) -> Self {
        let signing_key: SigningKey = private_key.into();
        let verifying_key = VerifyingKey::from(&signing_key);

        verifying_key.into()
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

#[cfg(feature = "serde")]
impl serde::Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_hex())
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Self::from_str(&s).map_err(serde::de::Error::custom)
        } else {
            let bytes = <Vec<u8>>::deserialize(deserializer)?;
            if bytes.len() != 33 {
                return Err(serde::de::Error::custom("Invalid public key length"));
            }
            let mut arr = [0u8; 33];
            arr.copy_from_slice(&bytes);
            Ok(Self(arr))
        }
    }
}

impl FromStr for PublicKey {
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        decode_hex_to_array(s).map(Self)
    }
}

fn decode_hex_to_array<const N: usize>(s: &str) -> Result<[u8; N], FromHexError> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    let mut buf = [0u8; N];
    hex::decode_to_slice(stripped, &mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anvil_default_account_matches_expected_address() {
        // Default Anvil account private key (see Hardhat/Foundry defaults)
        let private_key = PrivateKey::from_str(
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        )
        .unwrap();
        let public_key = PublicKey::from(private_key);
        assert_eq!(
            public_key.to_address().to_hex(),
            "f39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );
    }
}
