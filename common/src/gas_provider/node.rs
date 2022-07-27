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

/// Node of the ['Tree'] gas tree
#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub enum GasNode<ExternalId: Clone, Id: Clone, Balance: Zero + Clone> {
    /// A root node for each gas tree. Usually created when a new gas-ful logic is started (i.e., user's message is sent).
    External {
        id: ExternalId,
        value: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    /// A node created by "cutting" value from some other tree node. Such node types are independent and aren't
    /// part of the tree structure (not node's parent, not node's child).
    ReservedLocal { id: ExternalId, value: Balance },
    /// A node, which is a part of the tree structure (can be a parent and/or a child). As well as `External` node,
    /// it has an internal balance and can exist while being consumed (for more info read [`Tree::consume`] implementor docs).
    /// However, it has a `parent` field pointing to the node from which that one was created.
    SpecifiedLocal {
        parent: Id,
        value: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    /// Pretty same as `SpecifiedLocal`, but doesn't have internal balance, so relies on its `parent`. Also such
    /// nodes don't have children references.
    UnspecifiedLocal { parent: Id },
}

/// Children references convenience struct
#[derive(Clone, Copy, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct ChildrenRefs {
    spec_refs: u32,
    unspec_refs: u32,
}

impl<ExternalId: Clone, Id: Clone + Copy, Balance: Zero + Clone + Copy>
    GasNode<ExternalId, Id, Balance>
{
    /// Creates a new `GasNode::External` root node for a new tree.
    pub fn new(origin: ExternalId, value: Balance) -> Self {
        Self::External {
            id: origin,
            value,
            refs: Default::default(),
            consumed: false,
        }
    }

    /// Increases node's spec refs, if it can have any
    pub fn increase_spec_refs(&mut self) {
        self.adjust_refs(true, true);
    }

    /// Decreases node's spec refs, if it can have any
    pub fn decrease_spec_refs(&mut self) {
        self.adjust_refs(false, true);
    }

    /// Increases node's unspec refs, if it can have any
    pub fn increase_unspec_refs(&mut self) {
        self.adjust_refs(true, false);
    }

    /// Decreases node's unspec refs, if it can have any
    pub fn decrease_unspec_refs(&mut self) {
        self.adjust_refs(false, false);
    }

    /// Get's a mutable access to node's inner gas balance, if it can have any
    pub fn value_mut(&mut self) -> Option<&mut Balance> {
        match self {
            Self::External { ref mut value, .. }
            | Self::ReservedLocal { ref mut value, .. }
            | Self::SpecifiedLocal { ref mut value, .. } => Some(value),
            Self::UnspecifiedLocal { .. } => None,
        }
    }

    /// Marks the node as consumed, if it has the flag
    pub fn mark_consumed(&mut self) {
        if let Self::External { consumed, .. } | Self::SpecifiedLocal { consumed, .. } = self {
            *consumed = true;
        }
    }

    /// Returns whether the node is marked consumed or not
    ///
    /// Only `GasNode::External` and `GasNode::SpecifiedLocal` can be marked consumed and not deleted.
    /// For more info read [`Tree::consume`] implementor docs.
    pub fn is_consumed(&self) -> bool {
        if let Self::External { consumed, .. } | Self::SpecifiedLocal { consumed, .. } = self {
            *consumed
        } else {
            false
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
        if let Self::External { refs, consumed, .. } | Self::SpecifiedLocal { refs, consumed, .. } =
            self
        {
            !consumed || refs.unspec_refs != 0
        } else {
            false
        }
    }

    /// Returns node's inner gas balance, if it can have any
    pub fn value(&self) -> Option<Balance> {
        match self {
            Self::External { value, .. }
            | Self::ReservedLocal { value, .. }
            | Self::SpecifiedLocal { value, .. } => Some(*value),
            Self::UnspecifiedLocal { .. } => None,
        }
    }

    /// Returns node's parent, if it can have any.
    ///
    /// That is, `GasNode::External` and `GasNode::ReservedLocal` nodes
    /// don't have a parent, so a `None` is returned if the function is
    /// called on them.
    pub fn parent(&self) -> Option<Id> {
        match self {
            Self::External { .. } | Self::ReservedLocal { .. } => None,
            Self::SpecifiedLocal { parent, .. } | Self::UnspecifiedLocal { parent } => {
                Some(*parent)
            }
        }
    }

    /// Returns node's total refs
    pub fn refs(&self) -> u32 {
        self.spec_refs().saturating_add(self.unspec_refs())
    }

    /// Returns node's spec refs
    pub fn spec_refs(&self) -> u32 {
        match self {
            Self::External { refs, .. } | Self::SpecifiedLocal { refs, .. } => refs.spec_refs,
            _ => 0,
        }
    }

    /// Returns node's unspec refs
    pub fn unspec_refs(&self) -> u32 {
        match self {
            Self::External { refs, .. } | Self::SpecifiedLocal { refs, .. } => refs.unspec_refs,
            _ => 0,
        }
    }

    /// Returns whether the node is of `External` type
    pub(crate) fn is_external(&self) -> bool {
        matches!(self, Self::External { .. })
    }

    /// Returns whether the node is of `SpecifiedLocal` type
    pub(crate) fn is_specified_local(&self) -> bool {
        matches!(self, Self::SpecifiedLocal { .. })
    }

    /// Returns whether the node is of `UnspecifiedLocal` type
    pub(crate) fn is_unspecified_local(&self) -> bool {
        matches!(self, Self::UnspecifiedLocal { .. })
    }

    /// Returns whether the node is of `ReservedLocal` type
    pub(crate) fn is_reserved_local(&self) -> bool {
        matches!(self, Self::ReservedLocal { .. })
    }

    fn adjust_refs(&mut self, increase: bool, spec: bool) {
        if let Self::External { refs, .. } | Self::SpecifiedLocal { refs, .. } = self {
            match (increase, spec) {
                (true, true) => refs.spec_refs = refs.spec_refs.saturating_add(1),
                (true, false) => refs.unspec_refs = refs.unspec_refs.saturating_add(1),
                (false, true) => refs.spec_refs = refs.spec_refs.saturating_sub(1),
                (false, false) => refs.unspec_refs = refs.unspec_refs.saturating_sub(1),
            }
        }
    }
}
