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

use scale_info::TypeInfo;
use codec::{Decode, Encode};
use primitive_types::H256;
use crate::prelude::convert::TryFrom;
use crate::prelude::String;

#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode)]
pub struct MessageId([u8; 32]);

impl Into<gcore::MessageId> for MessageId {
    fn into(self) -> gcore::MessageId {
        gcore::MessageId(self.0)
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

#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq, TypeInfo, Decode, Encode)]
pub struct ActorId([u8; 32]);

impl ActorId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(arr)
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self, &'static str> {
        if slice.len() != 32 {
            return Err("Not enough len");
        }

        let mut actor_id: Self = Default::default();
        actor_id.0[..].copy_from_slice(slice);

        Ok(actor_id)
    }

    pub fn from_bs58(address: String) -> Result<ActorId, &'static str> {
        bs58::decode(address)
            .into_vec()
            .map(|v| ActorId::from_slice(&v[1..v.len() - 2].to_vec()))
            .map_err(|_| "")?
    }
}

impl From<[u8; 32]> for ActorId {
    fn from(arr: [u8; 32]) -> Self {
        Self::new(arr)
    }
}

impl TryFrom<&[u8]> for ActorId {
    type Error = &'static str;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        Self::from_slice(slice)
    }
}

impl From<H256> for ActorId {
    fn from(h256: H256) -> Self {
        Self::new(h256.to_fixed_bytes())
    }
}

impl From<gcore::ProgramId> for ActorId {
    fn from(other: gcore::ProgramId) -> Self {
        Self(other.0)
    }
}

impl Into<gcore::ProgramId> for ActorId {
    fn into(self) -> gcore::ProgramId {
        gcore::ProgramId(self.0)
    }
}

impl AsRef<[u8]> for ActorId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
