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

use crate::prelude::convert::TryFrom;
use crate::prelude::String;
use crate::errors::{Result, ContractError};
use codec::{Decode, Encode, Output};
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

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 32 {
            return Err(ContractError::Convert("Slice should be 32 length"));
        }

        let mut actor_id: Self = Default::default();
        actor_id.0[..].copy_from_slice(slice);

        Ok(actor_id)
    }

    pub fn from_bs58(address: String) -> Result<ActorId> {
        bs58::decode(address)
            .into_vec()
            .map(|v| ActorId::from_slice(&v[1..v.len() - 2].to_vec()))
            .map_err(|_| ContractError::Convert("Unable to decode bs58 address"))?
    }
}

impl From<[u8; 32]> for ActorId {
    fn from(arr: [u8; 32]) -> Self {
        Self::new(arr)
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

impl TryFrom<&[u8]> for ActorId {
    type Error = ContractError;

    fn try_from(slice: &[u8]) -> Result<Self> {
        Self::from_slice(slice)
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

impl AsRef<[u8]> for ActorId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle(gcore::MessageHandle);

impl MessageHandle {
    pub fn init() -> Self {
        crate::msg::send_init()
    }

    pub fn push<T: AsRef<[u8]>>(&self, payload: T) {
        crate::msg::send_push(self, payload);
    }

    pub fn commit(self, program: ActorId, gas_limit: u64, value: u128) -> MessageId {
        crate::msg::send_commit(self, program, gas_limit, value)
    }
}

impl AsRef<gcore::MessageHandle> for MessageHandle {
    fn as_ref(&self) -> &gcore::MessageHandle {
        &self.0
    }
}

impl From<gcore::MessageHandle> for MessageHandle {
    fn from(other: gcore::MessageHandle) -> Self {
        Self(other)
    }
}

impl From<MessageHandle> for gcore::MessageHandle {
    fn from(other: MessageHandle) -> Self {
        other.0
    }
}

impl Output for MessageHandle {
    fn write(&mut self, bytes: &[u8]) {
        self.push(bytes);
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

impl AsRef<[u8]> for MessageId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
