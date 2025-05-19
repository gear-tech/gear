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

//! Ethereum address.

use crate::PublicKey;
use anyhow::{anyhow, Error, Result};
use derive_more::{Debug, Display, From};
use gprimitives::{ActorId, H160};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;
use std::str::FromStr;

/// Ethereum address type.
///
/// Basically a 20 bytes buffer, which is obtained from the least significant 20 bytes
/// of the hashed with keccak256 public key.
#[derive(
    Encode, Decode, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, From, Debug, Display,
)]
#[from([u8; 20], H160)]
#[display("0x{}", self.to_hex())]
#[debug("0x{}", self.to_hex())]
pub struct Address(pub [u8; 20]);

impl Address {
    /// Address hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<PublicKey> for Address {
    fn from(key: PublicKey) -> Self {
        let public_key_uncompressed = key.to_uncompressed();

        let mut address = Address::default();
        let hash = sha3::Keccak256::digest(public_key_uncompressed);
        address.0[..20].copy_from_slice(&hash[12..]);

        address
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self(crate::decode_to_array(s)?))
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

impl From<u64> for Address {
    fn from(value: u64) -> Self {
        let actor_id = ActorId::from(value);
        actor_id
            .try_into()
            .expect("actor id from `u64` has first 12 bytes being 0")
    }
}

impl From<Address> for ActorId {
    fn from(value: Address) -> Self {
        H160(value.0).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_to_address() {
        // Does not panic
        let _ = Address::from(u64::MAX / 2);
    }

    #[test]
    fn try_from_actor_id() {
        let id =
            ActorId::from_str("0x0000000000000000000000006e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect("Must be correct ethereum address");

        let id =
            ActorId::from_str("0x1111111111111111111111116e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect_err("Must be incorrect ethereum address");
    }
}
