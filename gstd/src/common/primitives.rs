// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Gear primitive types.

use crate::errors::{ContractError, Result};
use crate::prelude::convert::TryFrom;
use crate::prelude::String;
use codec::{Decode, Encode};
use primitive_types::H256;
use scale_info::TypeInfo;

#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct ActorId([u8; 32]);

impl ActorId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub fn from_bs58(address: String) -> Result<Self> {
        bs58::decode(address)
            .into_vec()
            .map(|v| Self::from_slice(&v[1..v.len() - 2].to_vec()))
            .map_err(|_| ContractError::Convert("Unable to decode bs58 address"))?
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 32 {
            return Err(ContractError::Convert("Slice should be 32 length"));
        }

        let mut actor_id: Self = Default::default();
        actor_id.0[..].copy_from_slice(slice);

        Ok(actor_id)
    }
}

impl AsRef<[u8]> for ActorId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for ActorId {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

#[cfg(feature = "debug")]
impl From<u64> for ActorId {
    fn from(v: u64) -> Self {
        let mut arr = [0u8; 32];
        arr[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        Self(arr)
    }
}

impl From<[u8; 32]> for ActorId {
    fn from(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl From<ActorId> for [u8; 32] {
    fn from(other: ActorId) -> Self {
        other.0
    }
}

impl From<H256> for ActorId {
    fn from(h256: H256) -> Self {
        Self::new(h256.to_fixed_bytes())
    }
}

impl From<gcore::ActorId> for ActorId {
    fn from(other: gcore::ActorId) -> Self {
        Self(other.0)
    }
}

impl From<ActorId> for gcore::ActorId {
    fn from(other: ActorId) -> Self {
        Self(other.0)
    }
}

impl TryFrom<&[u8]> for ActorId {
    type Error = ContractError;

    fn try_from(slice: &[u8]) -> Result<Self> {
        Self::from_slice(slice)
    }
}

#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct MessageId([u8; 32]);

#[cfg(feature = "debug")]
impl MessageId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl AsRef<[u8]> for MessageId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<MessageId> for gcore::MessageId {
    fn from(other: MessageId) -> Self {
        Self(other.0)
    }
}

impl From<gcore::MessageId> for MessageId {
    fn from(other: gcore::MessageId) -> Self {
        Self(other.0)
    }
}

#[derive(
    Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode,
)]
pub struct CodeHash([u8; 32]);

impl CodeHash {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 32 {
            return Err(ContractError::Convert("Slice should be 32 length"));
        }

        let mut ret: Self = Default::default();
        ret.0.as_mut().copy_from_slice(slice);

        Ok(ret)
    }
}

impl AsRef<[u8]> for CodeHash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8]> for CodeHash {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.as_mut()
    }
}

impl From<[u8; 32]> for CodeHash {
    fn from(arr: [u8; 32]) -> Self {
        Self(arr)
    }
}

impl From<CodeHash> for [u8; 32] {
    fn from(other: CodeHash) -> Self {
        other.0
    }
}

impl From<H256> for CodeHash {
    fn from(h256: H256) -> Self {
        Self::new(h256.to_fixed_bytes())
    }
}

impl From<gcore::CodeHash> for CodeHash {
    fn from(other: gcore::CodeHash) -> Self {
        Self(other.0)
    }
}

impl From<CodeHash> for gcore::CodeHash {
    fn from(other: CodeHash) -> Self {
        Self(other.0)
    }
}

impl TryFrom<&[u8]> for CodeHash {
    type Error = ContractError;

    fn try_from(slice: &[u8]) -> Result<Self> {
        Self::from_slice(slice)
    }
}
