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

use super::keys::PublicKey;
use alloc::{string::String, vec::Vec};
use core::str::FromStr;
use derive_more::{Debug, Display, Error};
use gprimitives::{ActorId, H160};
use hex::FromHexError;
use nonempty::NonEmpty;
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

/// Ethereum address type.
///
/// Basically a 20 bytes buffer, which is obtained from the least significant 20 bytes
/// of the hashed with keccak256 public key.
#[derive(
    Encode,
    Decode,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::From,
    derive_more::Debug,
    derive_more::Display,
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

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
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
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, FromHexError> {
        crate::decode_to_array(s).map(Self)
    }
}

#[cfg(feature = "std")]
impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_hex().serialize(serializer)
    }
}

#[cfg(feature = "std")]
impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let address = String::deserialize(deserializer)?;
        let address = Address::from_str(&address).map_err(serde::de::Error::custom)?;
        Ok(address)
    }
}

#[derive(Debug, Display, Error)]
#[display("{:?}", self)]
#[debug("First 12 bytes are not 0, it is not ethereum address")]
pub struct FromActorIdError;

/// Tries to convert `ActorId`` into `Address`.
///
/// Succeeds if first 12 bytes are 0.
impl TryFrom<ActorId> for Address {
    type Error = FromActorIdError;

    fn try_from(id: ActorId) -> Result<Self, Self::Error> {
        id.as_ref()
            .iter()
            .take(12)
            .all(|&byte| byte == 0)
            .then_some(Address(id.to_address_lossy().0))
            .ok_or(FromActorIdError)
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

// Type conversions to/from `alloy_primitives::Address`
impl From<alloy_primitives::Address> for Address {
    fn from(value: alloy_primitives::Address) -> Self {
        Self(value.0.0)
    }
}

impl From<Address> for alloy_primitives::Address {
    fn from(value: Address) -> Self {
        Self(value.0.into())
    }
}

/// [`ValidatorsVec`] is a wrapper over non-empty vector of [`Address`].
/// It is needed because `NonEmpty` does not implement `Encode` and `Decode`.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::IntoIterator,
)]
pub struct ValidatorsVec(NonEmpty<Address>);

// parity-scale-codec Encode / Decode implementations
impl Encode for ValidatorsVec {
    fn encode(&self) -> Vec<u8> {
        Into::<Vec<_>>::into(self.0.clone()).encode()
    }
}

impl Decode for ValidatorsVec {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let inner: Vec<Address> = Decode::decode(input)?;
        NonEmpty::from_vec(inner)
            .map(Self)
            .ok_or(parity_scale_codec::Error::from(
                "Failed to decode ValidatorsVec: empty vector",
            ))
    }
}

#[derive(Debug, Display, Error)]
#[display("{:?}", self)]
#[debug("ValidatorsVec cannot be create from empty collection")]
pub struct EmptyValidatorsError;

// Useful conversions from / to `Vec<Address>`
impl TryFrom<Vec<Address>> for ValidatorsVec {
    type Error = EmptyValidatorsError;

    fn try_from(value: Vec<Address>) -> Result<Self, Self::Error> {
        NonEmpty::from_vec(value)
            .map(Self)
            .ok_or(EmptyValidatorsError)
    }
}

impl TryFrom<Vec<alloy_primitives::Address>> for ValidatorsVec {
    type Error = EmptyValidatorsError;

    fn try_from(value: Vec<alloy_primitives::Address>) -> Result<Self, Self::Error> {
        let vec: Vec<Address> = value.into_iter().map(Into::into).collect();
        NonEmpty::from_vec(vec)
            .map(Self)
            .ok_or(EmptyValidatorsError)
    }
}

impl FromIterator<Address> for Result<ValidatorsVec, EmptyValidatorsError> {
    fn from_iter<T: IntoIterator<Item = Address>>(iter: T) -> Self {
        let inner = iter.into_iter().collect::<Vec<_>>();
        inner.try_into()
    }
}

impl From<NonEmpty<Address>> for ValidatorsVec {
    fn from(value: NonEmpty<Address>) -> Self {
        Self(value)
    }
}

impl From<ValidatorsVec> for Vec<Address> {
    fn from(value: ValidatorsVec) -> Self {
        value.0.into()
    }
}

impl From<ValidatorsVec> for Vec<ActorId> {
    fn from(value: ValidatorsVec) -> Self {
        value.into_iter().map(Into::into).collect()
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
