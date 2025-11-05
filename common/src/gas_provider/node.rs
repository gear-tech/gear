// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use core::ops::{Add, Index, IndexMut};
use enum_iterator::cardinality;
use gear_core::ids::ReservationId;
use sp_runtime::{
    codec::{self, MaxEncodedLen},
    scale_info,
    traits::Zero,
};

/// ID of the [`GasNode`].
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum GasNodeId<T, U> {
    Node(T),
    Reservation(U),
}

impl<T, U> GasNodeId<T, U> {
    pub fn to_node_id(self) -> Option<T> {
        match self {
            GasNodeId::Node(message_id) => Some(message_id),
            GasNodeId::Reservation(_) => None,
        }
    }

    pub fn to_reservation_id(self) -> Option<U> {
        match self {
            GasNodeId::Node(_) => None,
            GasNodeId::Reservation(reservation_id) => Some(reservation_id),
        }
    }
}

impl<T, U> fmt::Display for GasNodeId<T, U>
where
    T: fmt::Display,
    U: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GasNodeId::Node(id) => fmt::Display::fmt(id, f),
            GasNodeId::Reservation(id) => fmt::Display::fmt(id, f),
        }
    }
}

impl<U> From<MessageId> for GasNodeId<MessageId, U> {
    fn from(id: MessageId) -> Self {
        Self::Node(id)
    }
}

impl<T> From<ReservationId> for GasNodeId<T, ReservationId> {
    fn from(id: ReservationId) -> Self {
        Self::Reservation(id)
    }
}

#[derive(Clone, Copy, Decode, Encode, Debug, Default, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct NodeLock<Balance>([Balance; cardinality::<LockId>()]);

impl<Balance> Index<LockId> for NodeLock<Balance> {
    type Output = Balance;

    fn index(&self, index: LockId) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl<Balance> IndexMut<LockId> for NodeLock<Balance> {
    fn index_mut(&mut self, index: LockId) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

impl<Balance: Zero + Copy> Zero for NodeLock<Balance> {
    fn zero() -> Self {
        Self([Balance::zero(); cardinality::<LockId>()])
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|x| x.is_zero())
    }
}

impl<Balance: Add<Output = Balance> + Copy> Add<Self> for NodeLock<Balance> {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        let NodeLock(mut inner) = self;
        let NodeLock(other) = other;

        for (i, elem) in inner.iter_mut().enumerate() {
            *elem = *elem + other[i];
        }

        Self(inner)
    }
}

// TODO: decide whether this method should stay or be removed as unused.
// The only use case currently is to check Gas Tree migration upon runtime upgrade.
impl<Balance: Zero + Copy + sp_runtime::traits::Saturating> NodeLock<Balance> {
    pub fn total_locked(&self) -> Balance {
        self.0
            .iter()
            .fold(Balance::zero(), |acc, v| acc.saturating_add(*v))
    }
}

/// Node of the ['Tree'] gas tree
#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum GasNode<ExternalId: Clone, Id: Clone, Balance: Zero + Clone, Funds> {
    /// A root node for each gas tree.
    ///
    /// Usually created when a new gasful logic started (i.e., message sent).
    External {
        id: ExternalId,
        multiplier: GasMultiplier<Funds, Balance>,
        value: Balance,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
        deposit: bool,
    },

    /// A node created by "cutting" value from some other tree node.
    ///
    /// Such node types are detached and aren't part of the tree structure
    /// (not node's parent, not node's child).
    Cut {
        id: ExternalId,
        multiplier: GasMultiplier<Funds, Balance>,
        value: Balance,
        lock: NodeLock<Balance>,
    },

    /// A node used for gas reservation feature.
    ///
    /// Such node types are detached from initial tree and may act the a root of new tree.
    Reserved {
        id: ExternalId,
        multiplier: GasMultiplier<Funds, Balance>,
        value: Balance,
        lock: NodeLock<Balance>,
        refs: ChildrenRefs,
        consumed: bool,
    },

    /// A node, which is a part of the tree structure, that can be
    /// a parent and/or a child.
    ///
    /// As well as `External` node, it has an internal balance and can exist
    /// while being consumed (see [`Tree::consume`] for details).
    ///
    /// However, it has a `parent` field pointing to the node,
    /// from which that one was created.
    SpecifiedLocal {
        parent: Id,
        root: Id,
        value: Balance,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },

    /// Pretty same as `SpecifiedLocal`, but doesn't have internal balance,
    /// so relies on its `parent`.
    ///
    /// Such nodes don't have children references.
    UnspecifiedLocal {
        parent: Id,
        root: Id,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
    },
}

/// Children references convenience struct
#[derive(Clone, Copy, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ChildrenRefs {
    spec_refs: u32,
    unspec_refs: u32,
}

impl<
    ExternalId: Clone,
    Id: Clone + Copy,
    Balance: Default + Zero + Clone + Copy + sp_runtime::traits::Saturating,
    Funds: Clone,
> GasNode<ExternalId, Id, Balance, Funds>
{
    /// Returns total gas value inside GasNode.
    pub fn total_value(&self) -> Balance {
        self.value()
            .unwrap_or_default()
            .saturating_add(self.lock().total_locked())
            .saturating_add(self.system_reserve().unwrap_or_default())
    }
}

impl<ExternalId: Clone, Id: Clone + Copy, Balance: Default + Zero + Clone + Copy, Funds: Clone>
    GasNode<ExternalId, Id, Balance, Funds>
{
    /// Creates a new `GasNode::External` root node for a new tree.
    pub fn new(
        id: ExternalId,
        multiplier: GasMultiplier<Funds, Balance>,
        value: Balance,
        deposit: bool,
    ) -> Self {
        Self::External {
            id,
            multiplier,
            value,
            lock: Zero::zero(),
            system_reserve: Zero::zero(),
            refs: Default::default(),
            consumed: false,
            deposit,
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

    /// Marks the node as consumed, if it has the flag
    pub fn mark_consumed(&mut self) {
        if let Self::External { consumed, .. }
        | Self::SpecifiedLocal { consumed, .. }
        | Self::Reserved { consumed, .. } = self
        {
            *consumed = true;
        }
    }

    /// Returns whether the node is marked consumed or not
    ///
    /// Only `GasNode::External`, `GasNode::SpecifiedLocal`, `GasNode::Reserved` can be marked
    /// consumed and not deleted. See [`Tree::consume`] for details.
    pub fn is_consumed(&self) -> bool {
        if let Self::External { consumed, .. }
        | Self::SpecifiedLocal { consumed, .. }
        | Self::Reserved { consumed, .. } = self
        {
            *consumed
        } else {
            false
        }
    }

    /// Returns whether the node is patron or not.
    ///
    /// The flag signals whether the node isn't available
    /// for the gas to be spent from it.
    ///
    /// These are nodes that have one of the following requirements:
    /// 1. Have unspec refs (regardless of being consumed).
    /// 2. Are not consumed.
    ///
    /// Patron nodes are those on which other nodes of the tree rely
    /// (including the self node).
    pub fn is_patron(&self) -> bool {
        if let Self::External { refs, consumed, .. }
        | Self::SpecifiedLocal { refs, consumed, .. }
        | Self::Reserved { refs, consumed, .. } = self
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
            | Self::Cut { value, .. }
            | Self::Reserved { value, .. }
            | Self::SpecifiedLocal { value, .. } => Some(*value),
            Self::UnspecifiedLocal { .. } => None,
        }
    }

    /// Get's a mutable access to node's inner gas balance, if it can have any
    pub fn value_mut(&mut self) -> Option<&mut Balance> {
        match *self {
            Self::External { ref mut value, .. }
            | Self::Cut { ref mut value, .. }
            | Self::Reserved { ref mut value, .. }
            | Self::SpecifiedLocal { ref mut value, .. } => Some(value),
            Self::UnspecifiedLocal { .. } => None,
        }
    }

    /// Returns node's locked gas balance, if it can have any.
    pub fn lock(&self) -> &NodeLock<Balance> {
        match self {
            Self::External { lock, .. }
            | Self::UnspecifiedLocal { lock, .. }
            | Self::SpecifiedLocal { lock, .. }
            | Self::Reserved { lock, .. }
            | Self::Cut { lock, .. } => lock,
        }
    }

    /// Get's a mutable access to node's locked gas balance, if it can have any.
    pub fn lock_mut(&mut self) -> &mut NodeLock<Balance> {
        match *self {
            Self::External { ref mut lock, .. }
            | Self::UnspecifiedLocal { ref mut lock, .. }
            | Self::SpecifiedLocal { ref mut lock, .. }
            | Self::Reserved { ref mut lock, .. }
            | Self::Cut { ref mut lock, .. } => lock,
        }
    }

    /// Returns node's system reserved gas balance, if it can have any.
    pub fn system_reserve(&self) -> Option<Balance> {
        match self {
            GasNode::External { system_reserve, .. }
            | GasNode::SpecifiedLocal { system_reserve, .. }
            | GasNode::UnspecifiedLocal { system_reserve, .. } => Some(*system_reserve),
            GasNode::Cut { .. } | GasNode::Reserved { .. } => None,
        }
    }

    /// Gets a mutable access to node's system reserved gas balance, if it can have any.
    pub fn system_reserve_mut(&mut self) -> Option<&mut Balance> {
        match self {
            GasNode::External { system_reserve, .. }
            | GasNode::SpecifiedLocal { system_reserve, .. }
            | GasNode::UnspecifiedLocal { system_reserve, .. } => Some(system_reserve),
            GasNode::Cut { .. } | GasNode::Reserved { .. } => None,
        }
    }

    /// Returns node's parent, if it can have any.
    ///
    /// That is, `GasNode::External`, `GasNode::Cut`, 'GasNode::Reserved` nodes
    /// don't have a parent, so a `None` is returned if the function is
    /// called on them.
    pub fn parent(&self) -> Option<Id> {
        match self {
            Self::External { .. } | Self::Cut { .. } | Self::Reserved { .. } => None,
            Self::SpecifiedLocal { parent, .. } | Self::UnspecifiedLocal { parent, .. } => {
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
            Self::External { refs, .. }
            | Self::SpecifiedLocal { refs, .. }
            | Self::Reserved { refs, .. } => refs.spec_refs,
            _ => 0,
        }
    }

    /// Returns node's unspec refs
    pub fn unspec_refs(&self) -> u32 {
        match self {
            Self::External { refs, .. }
            | Self::SpecifiedLocal { refs, .. }
            | Self::Reserved { refs, .. } => refs.unspec_refs,
            _ => 0,
        }
    }

    /// Returns id of the root node.
    pub fn root_id(&self) -> Option<Id> {
        match self {
            Self::SpecifiedLocal { root, .. } | Self::UnspecifiedLocal { root, .. } => Some(*root),
            Self::External { .. } | Self::Cut { .. } | Self::Reserved { .. } => None,
        }
    }

    /// Returns external origin and funds multiplier of the node if contains that data inside.
    pub fn external_data(&self) -> Option<(ExternalId, GasMultiplier<Funds, Balance>)> {
        match self {
            Self::External { id, multiplier, .. }
            | Self::Cut { id, multiplier, .. }
            | Self::Reserved { id, multiplier, .. } => Some((id.clone(), multiplier.clone())),
            Self::SpecifiedLocal { .. } | Self::UnspecifiedLocal { .. } => None,
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

    /// Returns whether the node is of `Cut` type
    pub(crate) fn is_cut(&self) -> bool {
        matches!(self, Self::Cut { .. })
    }

    /// Returns whether the node is of `Reserve` type
    pub(crate) fn is_reserved(&self) -> bool {
        matches!(self, Self::Reserved { .. })
    }

    /// Returns whether the node has system reserved gas.
    pub(crate) fn is_system_reservable(&self) -> bool {
        self.system_reserve().is_some()
    }

    fn adjust_refs(&mut self, increase: bool, spec: bool) {
        if let Self::External { refs, .. }
        | Self::SpecifiedLocal { refs, .. }
        | Self::Reserved { refs, .. } = self
        {
            match (increase, spec) {
                (true, true) => refs.spec_refs = refs.spec_refs.saturating_add(1),
                (true, false) => refs.unspec_refs = refs.unspec_refs.saturating_add(1),
                (false, true) => refs.spec_refs = refs.spec_refs.saturating_sub(1),
                (false, false) => refs.unspec_refs = refs.unspec_refs.saturating_sub(1),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Checking node that have external data do not have root id
    fn asserts_node_have_either_external_data_or_root_id() {
        let nodes_with_external_data: [gas_provider::node::GasNode<i32, i32, i32, i32>; 3] = [
            GasNode::External {
                id: Default::default(),
                multiplier: GasMultiplier::ValuePerGas(100),
                value: Default::default(),
                lock: Default::default(),
                system_reserve: Default::default(),
                refs: Default::default(),
                consumed: Default::default(),
                deposit: Default::default(),
            },
            GasNode::Cut {
                id: Default::default(),
                multiplier: GasMultiplier::ValuePerGas(100),
                value: Default::default(),
                lock: Default::default(),
            },
            GasNode::Reserved {
                id: Default::default(),
                multiplier: GasMultiplier::ValuePerGas(100),
                value: Default::default(),
                lock: Default::default(),
                refs: Default::default(),
                consumed: Default::default(),
            },
        ];

        for node in nodes_with_external_data {
            assert!(node.external_data().is_some() || node.root_id().is_none());
        }

        let nodes_with_root_id: [gas_provider::node::GasNode<i32, i32, i32, i32>; 2] = [
            GasNode::SpecifiedLocal {
                parent: Default::default(),
                root: Default::default(),
                value: Default::default(),
                lock: Default::default(),
                system_reserve: Default::default(),
                refs: Default::default(),
                consumed: Default::default(),
            },
            GasNode::UnspecifiedLocal {
                parent: Default::default(),
                root: Default::default(),
                lock: Default::default(),
                system_reserve: Default::default(),
            },
        ];

        for node in nodes_with_root_id {
            assert!(node.external_data().is_none() || node.root_id().is_some());
        }
    }
}
