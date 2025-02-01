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

//! Ethereum address.

use crate::{utils, PublicKey};
use anyhow::{anyhow, Error, Result};
use gprimitives::{ActorId, H160};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;
use std::{fmt, str::FromStr};

/// Ethereum address type.
///
/// Basically a 20 bytes buffer, which is obtained from the least significant 20 bytes
/// of the hashed with keccak256 public key.
#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address(pub [u8; 20]);

impl Address {
    /// Address hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<[u8; 20]> for Address {
    fn from(value: [u8; 20]) -> Self {
        Self(value)
    }
}

impl From<H160> for Address {
    fn from(value: H160) -> Self {
        Self(value.into())
    }
}

impl From<PublicKey> for Address {
    fn from(key: PublicKey) -> Self {
        let public_key_uncompressed = secp256k1::PublicKey::from(key).serialize_uncompressed();

        let mut address = Address::default();
        let hash = sha3::Keccak256::digest(&public_key_uncompressed[1..]);
        address.0[..20].copy_from_slice(&hash[12..]);

        address
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self(utils::decode_to_array(s)?))
    }
}

/// Tries to convert `ActorId`` into `Address`.
///
/// Succeeds if first 12 bytes are 0.
impl TryFrom<ActorId> for Address {
    type Error = Error;

    fn try_from(id: ActorId) -> Result<Self> {
        id.as_ref()
            .iter()
            .take(12)
            .all(|&byte| byte == 0)
            .then_some(Address(id.to_address_lossy().0))
            .ok_or_else(|| anyhow!("First 12 bytes are not 0, it is not ethereum address"))
    }
}

impl From<Address> for ActorId {
    fn from(value: Address) -> Self {
        H160(value.0).into()
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}
