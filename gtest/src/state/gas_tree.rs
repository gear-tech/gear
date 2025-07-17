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

//! Auxiliary (for tests) gas tree management implementation for the crate.

use std::{
    cell::{Ref, RefMut},
    collections::BTreeMap,
};

use crate::{GAS_MULTIPLIER, state::WithOverlay};
use gear_common::{
    Gas, GasMultiplier, LockId, Origin,
    gas_provider::{
        ConsumeResultOf, GasNodeId, LockableTree, Provider, ReservableTree, Tree,
        auxiliary::{
            AuxiliaryGasProvider, GasNodesProvider, GasNodesStorage, GasTreeError, Node, NodeId,
            PlainNodeId, TotalIssuanceProvider, TotalIssuanceStorage,
        },
    },
};
use gear_core::ids::{ActorId, MessageId, ReservationId};

pub(crate) type PositiveImbalance = <GasTree as Tree>::PositiveImbalance;
pub(crate) type NegativeImbalance = <GasTree as Tree>::NegativeImbalance;
pub type OriginNodeDataOf = (
    <GasTree as Tree>::ExternalOrigin,
    GasMultiplier<<GasTree as Tree>::Funds, <GasTree as Tree>::Balance>,
    <GasTree as Tree>::NodeId,
);
type GtestGasProvider = AuxiliaryGasProvider<
    GTestTotalIssuanceStorage,
    GTestTotalIssuanceProvider,
    GTestGasNodesStorage,
    GTestGasNodesProvider,
>;
type GasTree = <GtestGasProvider as Provider>::GasTree;

std::thread_local! {
    pub(super) static GTEST_TOTAL_ISSUANCE: GTestTotalIssuanceProvider = Default::default();
    pub(super) static GTEST_GAS_NODES: GTestGasNodesProvider = Default::default();
}

pub struct GTestTotalIssuanceStorage;
pub type GTestTotalIssuanceProvider = WithOverlay<Option<Gas>>;

impl TotalIssuanceStorage<GTestTotalIssuanceProvider> for GTestTotalIssuanceStorage {
    fn storage() -> &'static std::thread::LocalKey<GTestTotalIssuanceProvider> {
        &GTEST_TOTAL_ISSUANCE
    }
}

impl TotalIssuanceProvider for GTestTotalIssuanceProvider {
    fn data(&self) -> Ref<'_, Option<Gas>> {
        self.data()
    }

    fn data_mut(&self) -> RefMut<'_, Option<Gas>> {
        self.data_mut()
    }
}

pub struct GTestGasNodesStorage;
pub type GTestGasNodesProvider = WithOverlay<BTreeMap<NodeId, Node>>;

impl GasNodesStorage<GTestGasNodesProvider> for GTestGasNodesStorage {
    fn storage() -> &'static std::thread::LocalKey<GTestGasNodesProvider> {
        &GTEST_GAS_NODES
    }
}

impl GasNodesProvider for GTestGasNodesProvider {
    fn data(&self) -> Ref<'_, BTreeMap<NodeId, Node>> {
        self.data()
    }

    fn data_mut(&self) -> RefMut<'_, BTreeMap<NodeId, Node>> {
        self.data_mut()
    }
}

#[derive(Debug, Default)]
pub(crate) struct GasTreeManager;

impl GasTreeManager {
    /// Adapted by argument types version of the gas tree `create` method.
    pub(crate) fn create(
        &self,
        origin: ActorId,
        mid: MessageId,
        amount: Gas,
        is_reply: bool,
    ) -> Result<PositiveImbalance, GasTreeError> {
        if !is_reply || !self.exists_and_deposit(mid) {
            GasTree::create(
                origin.cast(),
                GAS_MULTIPLIER,
                GasNodeId::from(mid.cast::<PlainNodeId>()),
                amount,
            )
        } else {
            Ok(PositiveImbalance::new(0))
        }
    }

    /// Adapted by argument types version of the gas tree `create_deposit`
    /// method.
    pub(crate) fn create_deposit(
        &self,
        original_mid: MessageId,
        future_reply_id: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        GasTree::create_deposit(
            GasNodeId::from(original_mid.cast::<PlainNodeId>()),
            GasNodeId::from(future_reply_id.cast::<PlainNodeId>()),
            amount,
        )
    }

    /// Adapted by argument types version of the gas tree `split_with_value`
    /// method.
    pub(crate) fn split_with_value(
        &self,
        is_reply: bool,
        original_mid: impl Origin,
        new_mid: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        if !is_reply || !GasTree::exists_and_deposit(GasNodeId::from(new_mid.cast::<PlainNodeId>()))
        {
            return GasTree::split_with_value(
                GasNodeId::from(original_mid.cast::<PlainNodeId>()),
                GasNodeId::from(new_mid.cast::<PlainNodeId>()),
                amount,
            );
        }

        Ok(())
    }

    /// Adapted by argument types version of the gas tree `split` method.
    pub(crate) fn split(
        &self,
        is_reply: bool,
        original_node: impl Origin,
        new_mid: MessageId,
    ) -> Result<(), GasTreeError> {
        if !is_reply || !GasTree::exists_and_deposit(GasNodeId::from(new_mid.cast::<PlainNodeId>()))
        {
            return GasTree::split(
                GasNodeId::from(original_node.cast::<PlainNodeId>()),
                GasNodeId::from(new_mid.cast::<PlainNodeId>()),
            );
        }

        Ok(())
    }

    /// Adapted by argument types version of the gas tree `cut` method.
    pub(crate) fn cut(
        &self,
        original_node: impl Origin,
        new_mid: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        GasTree::cut(
            GasNodeId::from(original_node.cast::<PlainNodeId>()),
            GasNodeId::from(new_mid.cast::<PlainNodeId>()),
            amount,
        )
    }

    /// Adapted by argument types version of the gas tree `get_limit` method.
    pub(crate) fn get_limit(&self, mid: impl Origin) -> Result<Gas, GasTreeError> {
        GasTree::get_limit(GasNodeId::from(mid.cast::<PlainNodeId>()))
    }

    /// Adapted by argument types version of the gas tree `spend` method.
    pub(crate) fn spend(
        &self,
        mid: MessageId,
        amount: Gas,
    ) -> Result<NegativeImbalance, GasTreeError> {
        GasTree::spend(GasNodeId::from(mid.cast::<PlainNodeId>()), amount)
    }

    /// Adapted by argument types version of the gas tree `consume` method.
    pub(crate) fn consume(&self, mid: impl Origin) -> ConsumeResultOf<GasTree> {
        GasTree::consume(GasNodeId::from(mid.cast::<PlainNodeId>()))
    }

    pub(crate) fn reserve_gas(
        &self,
        original_mid: MessageId,
        reservation_id: ReservationId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        GasTree::reserve(
            GasNodeId::from(original_mid.cast::<PlainNodeId>()),
            GasNodeId::from(reservation_id.cast::<PlainNodeId>()),
            amount,
        )
    }

    #[cfg(test)]
    pub(crate) fn exists(&self, node_id: impl Origin) -> bool {
        GasTree::exists(GasNodeId::from(node_id.cast::<PlainNodeId>()))
    }

    pub(crate) fn exists_and_deposit(&self, node_id: impl Origin) -> bool {
        GasTree::exists_and_deposit(GasNodeId::from(node_id.cast::<PlainNodeId>()))
    }

    /// Adapted by argument types version of the gas tree `reset` method.
    ///
    /// *Note* Call with caution as it completely resets the storage.
    pub(crate) fn clear(&self) {
        <GtestGasProvider as Provider>::reset();
    }

    /// Unreserve some value from underlying balance.
    ///
    /// Used in gas reservation for system signal.
    pub(crate) fn system_unreserve(&self, message_id: MessageId) -> Result<Gas, GasTreeError> {
        GasTree::system_unreserve(GasNodeId::from(message_id.cast::<PlainNodeId>()))
    }

    /// Reserve some value from underlying balance.
    ///
    /// Used in gas reservation for system signal.
    pub(crate) fn system_reserve(
        &self,
        message_id: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        GasTree::system_reserve(GasNodeId::from(message_id.cast::<PlainNodeId>()), amount)
    }

    pub fn lock(&self, node_id: impl Origin, id: LockId, amount: Gas) -> Result<(), GasTreeError> {
        GasTree::lock(GasNodeId::from(node_id.cast::<PlainNodeId>()), id, amount)
    }

    pub(crate) fn unlock_all(
        &self,
        message_id: impl Origin,
        id: LockId,
    ) -> Result<Gas, GasTreeError> {
        GasTree::unlock_all(GasNodeId::from(message_id.cast::<PlainNodeId>()), id)
    }

    /// The id of node, external origin and funds multiplier for a key.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    pub(crate) fn get_origin_node(
        &self,
        message_id: MessageId,
    ) -> Result<OriginNodeDataOf, GasTreeError> {
        GasTree::get_origin_node(GasNodeId::from(message_id.cast::<PlainNodeId>()))
    }
}
