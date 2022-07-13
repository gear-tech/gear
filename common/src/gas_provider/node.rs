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

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub enum GasNodeType<ExternalId, Id, Balance> {
    External { id: ExternalId, value: Balance },
    ReservedLocal { id: ExternalId, value: Balance },
    SpecifiedLocal { parent: Id, value: Balance },
    UnspecifiedLocal { parent: Id },
}

impl<ExternalId, Id, Balance> GasNodeType<ExternalId, Id, Balance> {
    pub(crate) fn is_external(&self) -> bool {
        matches!(self, GasNodeType::External { .. })
    }

    pub(crate) fn is_specified_local(&self) -> bool {
        matches!(self, GasNodeType::SpecifiedLocal { .. })
    }

    pub(crate) fn is_unspecified_local(&self) -> bool {
        matches!(self, GasNodeType::UnspecifiedLocal { .. })
    }

    pub(crate) fn is_reserved_local(&self) -> bool {
        matches!(self, GasNodeType::ReservedLocal { .. })
    }
}

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct GasNode<ExternalId: Clone, Id: Clone, Balance: Zero + Clone> {
    pub spec_refs: u32,
    pub unspec_refs: u32,
    pub inner: GasNodeType<ExternalId, Id, Balance>,
    pub consumed: bool,
}

impl<ExternalId: Clone, Id: Clone + Copy, Balance: Zero + Clone + Copy>
    GasNode<ExternalId, Id, Balance>
{
    pub fn new(origin: ExternalId, value: Balance) -> Self {
        Self {
            inner: GasNodeType::External { id: origin, value },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        }
    }

    /// Returns whether the node is patron or not
    ///
    /// The flag signals whether the node isn't available for the gas to be spent from it. These are nodes that:
    /// 1. Have unspec refs (regardless of being consumed).
    /// 2. Are not consumed.
    ///
    /// Patron nodes are those on which other nodes of the tree rely (including the self node).
    pub fn is_patron(&self) -> bool {
        (self.inner.is_external() || self.inner.is_specified_local())
            && (!self.consumed || self.unspec_refs != 0)
    }

    pub fn inner_value(&self) -> Option<Balance> {
        match self.inner {
            GasNodeType::External { value, .. } => Some(value),
            GasNodeType::ReservedLocal { value, .. } => Some(value),
            GasNodeType::SpecifiedLocal { value, .. } => Some(value),
            GasNodeType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn inner_value_mut(&mut self) -> Option<&mut Balance> {
        match self.inner {
            GasNodeType::External { ref mut value, .. } => Some(value),
            GasNodeType::ReservedLocal { ref mut value, .. } => Some(value),
            GasNodeType::SpecifiedLocal { ref mut value, .. } => Some(value),
            GasNodeType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn parent(&self) -> Option<Id> {
        match self.inner {
            GasNodeType::External { .. } | GasNodeType::ReservedLocal { .. } => None,
            GasNodeType::SpecifiedLocal { parent, .. }
            | GasNodeType::UnspecifiedLocal { parent } => Some(parent),
        }
    }

    pub fn refs(&self) -> u32 {
        self.spec_refs.saturating_add(self.unspec_refs)
    }
}
