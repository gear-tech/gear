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

//! Auxiliary implementation of the gas provider.

use super::overlay;
use overlay::WithOverlay;
use crate::{
    gas_provider::{Error, GasNode, GasNodeId, Provider, TreeImpl},
    storage::{MapStorage, ValueStorage},
    Origin,
};
use alloc::collections::BTreeMap;
use sp_core::H256;
use std::thread::LocalKey;

/// Balance type used in the gas tree.
pub(crate) type Balance = u64;
/// Type represents token value equivalent of gas.
pub(crate) type Funds = u128;
/// Type represents gas tree node id, which is a key for gas nodes map storage.
pub(crate) type NodeId = GasNodeId<PlainNodeId, ReservationNodeId>;
/// Type represents gas tree node, which is a value for gas nodes map storage.
pub(crate) type Node = GasNode<ExternalOrigin, NodeId, Balance, Funds>;

/// Gas provider implementor used in native, non-wasm runtimes.
pub struct AuxiliaryGasProvider;

impl Provider for AuxiliaryGasProvider {
    type ExternalOrigin = ExternalOrigin;
    type NodeId = NodeId;
    type Balance = Balance;
    type Funds = Funds;
    type InternalError = GasTreeError;
    type Error = GasTreeError;

    type GasTree = TreeImpl<
        TotalIssuanceWrap,
        Self::InternalError,
        Self::Error,
        ExternalOrigin,
        Self::NodeId,
        GasNodesWrap,
    >;
}

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExternalOrigin(pub H256);

impl Origin for ExternalOrigin {
    fn into_origin(self) -> H256 {
        self.0
    }

    fn from_origin(val: H256) -> Self {
        Self(val)
    }
}

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PlainNodeId(pub H256);

impl Origin for PlainNodeId {
    fn into_origin(self) -> H256 {
        self.0
    }

    fn from_origin(val: H256) -> Self {
        Self(val)
    }
}

impl<U> From<PlainNodeId> for GasNodeId<PlainNodeId, U> {
    fn from(plain_node_id: PlainNodeId) -> Self {
        Self::Node(plain_node_id)
    }
}

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ReservationNodeId(pub H256);

impl Origin for ReservationNodeId {
    fn into_origin(self) -> H256 {
        self.0
    }

    fn from_origin(val: H256) -> Self {
        Self(val)
    }
}

impl<T> From<ReservationNodeId> for GasNodeId<T, ReservationNodeId> {
    fn from(node_id: ReservationNodeId) -> Self {
        Self::Reservation(node_id)
    }
}

/// Error type serving error variants returned from gas tree methods.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GasTreeError {
    NodeAlreadyExists,
    ParentIsLost,
    ParentHasNoChildren,
    NodeNotFound,
    NodeWasConsumed,
    InsufficientBalance,
    Forbidden,
    UnexpectedConsumeOutput,
    UnexpectedNodeType,
    ValueIsNotCaught,
    ValueIsBlocked,
    ValueIsNotBlocked,
    ConsumedWithLock,
    ConsumedWithSystemReservation,
    TotalValueIsOverflowed,
    TotalValueIsUnderflowed,
}

impl Error for GasTreeError {
    fn node_already_exists() -> Self {
        Self::NodeAlreadyExists
    }

    fn parent_is_lost() -> Self {
        Self::ParentIsLost
    }

    fn parent_has_no_children() -> Self {
        Self::ParentHasNoChildren
    }

    fn node_not_found() -> Self {
        Self::NodeNotFound
    }

    fn node_was_consumed() -> Self {
        Self::NodeWasConsumed
    }

    fn insufficient_balance() -> Self {
        Self::InsufficientBalance
    }

    fn forbidden() -> Self {
        Self::Forbidden
    }

    fn unexpected_consume_output() -> Self {
        Self::UnexpectedConsumeOutput
    }

    fn unexpected_node_type() -> Self {
        Self::UnexpectedNodeType
    }

    fn value_is_not_caught() -> Self {
        Self::ValueIsNotCaught
    }

    fn value_is_blocked() -> Self {
        Self::ValueIsBlocked
    }

    fn value_is_not_blocked() -> Self {
        Self::ValueIsNotBlocked
    }

    fn consumed_with_lock() -> Self {
        Self::ConsumedWithLock
    }

    fn consumed_with_system_reservation() -> Self {
        Self::ConsumedWithSystemReservation
    }

    fn total_value_is_overflowed() -> Self {
        Self::TotalValueIsOverflowed
    }

    fn total_value_is_underflowed() -> Self {
        Self::TotalValueIsUnderflowed
    }
}

std::thread_local! {
    // Definition of the `TotalIssuance` global storage, accessed by the tree.
    pub(crate) static TOTAL_ISSUANCE: WithOverlay<Option<Balance>> = Default::default();
}

fn total_issuance_storage() -> &'static LocalKey<WithOverlay<Option<Balance>>> {
    &TOTAL_ISSUANCE
}

/// Global `TotalIssuance` storage manager.
#[derive(Debug, PartialEq, Eq)]
pub struct TotalIssuanceWrap;

impl ValueStorage for TotalIssuanceWrap {
    type Value = Balance;

    fn exists() -> bool {
        total_issuance_storage().with(|i| {
            i.data()
                .is_some()
        })
    }

    fn get() -> Option<Self::Value> {
        total_issuance_storage().with(|i| {
            *i.data()
        })
    }

    fn kill() {
        total_issuance_storage().with(|i| {
            let mut data = i.data_mut();
            *data = None;
        });
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R {
        total_issuance_storage().with(|i| {
            f(&mut i.data_mut())
        })
    }

    fn put(value: Self::Value) {
        total_issuance_storage().with(|i| {
            i.data_mut()
                .replace(value);
        });
    }

    fn set(value: Self::Value) -> Option<Self::Value> {
        Self::mutate(|opt| {
            let prev = opt.take();
            *opt = Some(value);
            prev
        })
    }

    fn take() -> Option<Self::Value> {
        total_issuance_storage().with(|i| {
            i.data_mut()
                .take()
        })
    }
}

std::thread_local! {
    // Definition of the `GasNodes` (tree `StorageMap`) global storage, accessed by the tree.
    pub(crate) static GAS_NODES: WithOverlay<BTreeMap<NodeId, Node>> = Default::default();
}

fn gas_nodes_storage() -> &'static LocalKey<WithOverlay<BTreeMap<NodeId, Node>>> {
    &GAS_NODES
}

/// Global `GasNodes` storage manager.
pub struct GasNodesWrap;

impl MapStorage for GasNodesWrap {
    type Key = NodeId;
    type Value = Node;

    fn contains_key(key: &Self::Key) -> bool {
        gas_nodes_storage().with(|tree| tree.data().contains_key(key))
    }

    fn get(key: &Self::Key) -> Option<Self::Value> {
        gas_nodes_storage().with(|tree| tree.data().get(key).cloned())
    }

    fn insert(key: Self::Key, value: Self::Value) {
        gas_nodes_storage().with(|tree| {
            tree.data_mut().insert(key, value);
        });
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(_key: Self::Key, _f: F) -> R {
        unimplemented!()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut _f: F) {
        unimplemented!()
    }

    fn remove(key: Self::Key) {
        gas_nodes_storage().with(|tree| {
            tree.data_mut().remove(&key);
        });
    }

    fn clear() {
        gas_nodes_storage().with(|tree| {
            tree.data_mut().clear();
        });
    }

    fn take(key: Self::Key) -> Option<Self::Value> {
        gas_nodes_storage().with(|tree| tree.data_mut().remove(&key))
    }
}