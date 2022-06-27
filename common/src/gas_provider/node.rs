// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use super::*;
use codec::MaxEncodedLen;

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub enum ValueType<ExternalId, Id, Balance> {
    External { id: ExternalId, value: Balance },
    ReservedLocal { id: ExternalId, value: Balance },
    SpecifiedLocal { parent: Id, value: Balance },
    UnspecifiedLocal { parent: Id },
}

impl<ExternalId: Default, Id, Balance: Zero> Default for ValueType<ExternalId, Id, Balance> {
    fn default() -> Self {
        ValueType::External {
            id: Default::default(),
            value: Zero::zero(),
        }
    }
}

#[derive(Clone, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub struct ValueNode<ExternalId: Default + Clone, Id: Clone, Balance: Zero + Clone> {
    pub spec_refs: u32,
    pub unspec_refs: u32,
    pub inner: ValueType<ExternalId, Id, Balance>,
    pub consumed: bool,
}

impl<ExternalId: Default + Clone, Id: Clone + Copy, Balance: Zero + Clone + Copy>
    ValueNode<ExternalId, Id, Balance>
{
    pub fn new(origin: ExternalId, value: Balance) -> Self {
        Self {
            inner: ValueType::External { id: origin, value },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        }
    }

    pub fn inner_value(&self) -> Option<Balance> {
        match self.inner {
            ValueType::External { value, .. } => Some(value),
            ValueType::ReservedLocal { value, .. } => Some(value),
            ValueType::SpecifiedLocal { value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn inner_value_mut(&mut self) -> Option<&mut Balance> {
        match self.inner {
            ValueType::External { ref mut value, .. } => Some(value),
            ValueType::ReservedLocal { ref mut value, .. } => Some(value),
            ValueType::SpecifiedLocal { ref mut value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn parent(&self) -> Option<Id> {
        match self.inner {
            ValueType::External { .. } | ValueType::ReservedLocal { .. } => None,
            ValueType::SpecifiedLocal { parent, .. } | ValueType::UnspecifiedLocal { parent } => {
                Some(parent)
            }
        }
    }

    pub fn refs(&self) -> u32 {
        self.spec_refs.saturating_add(self.unspec_refs)
    }
}
