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

#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct MessageId(pub(crate) gcore::MessageId);

impl From<gcore::MessageId> for MessageId {
    fn from(other: gcore::MessageId) -> Self {
        Self(other)
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, Ord, PartialEq, PartialOrd, Eq)]
pub struct ActorId(pub(crate) gcore::ProgramId);

impl From<u64> for ActorId {
    fn from(other: u64) -> Self {
        Self(other.into())
    }
}

impl ActorId {
    pub const fn new(arr: [u8; 32]) -> Self {
        Self(gcore::ProgramId(arr))
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn from_slice(slice: &[u8]) -> Self {
        Self(gcore::ProgramId::from_slice(slice))
    }
}
