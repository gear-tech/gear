// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::GAS_MULTIPLIER;
use gear_common::{
    auxiliary::gas_provider::{AuxiliaryGasProvider, GasTreeError, PlainNodeId},
    gas_provider::{ConsumeResultOf, GasNodeId, Provider, Tree},
    Gas, Origin,
};
use gear_core::ids::{MessageId, ProgramId};

pub(crate) type PositiveImbalance = <GasTree as Tree>::PositiveImbalance;
pub(crate) type NegativeImbalance = <GasTree as Tree>::NegativeImbalance;
type GasTree = <AuxiliaryGasProvider as Provider>::GasTree;

/// Gas tree manager which uses operates under the hood over
/// [`gear_common::AuxiliaryGasProvider`].
///
/// Manager is needed mainly to adapt arguments of the gas tree methods to the
/// crate.
#[derive(Debug, Default)]
pub(crate) struct GasTreeManager;

impl GasTreeManager {
    /// Adapted by argument types version of the gas tree `create` method.
    pub(crate) fn create(
        &self,
        origin: ProgramId,
        mid: MessageId,
        amount: Gas,
    ) -> Result<PositiveImbalance, GasTreeError> {
        GasTree::create(
            origin.cast(),
            GAS_MULTIPLIER,
            GasNodeId::from(mid.cast::<PlainNodeId>()),
            amount,
        )
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
        original_mid: MessageId,
        new_mid: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        if !is_reply && !GasTree::exists_and_deposit(GasNodeId::from(new_mid.cast::<PlainNodeId>()))
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
        original_mid: MessageId,
        new_mid: MessageId,
    ) -> Result<(), GasTreeError> {
        if !is_reply && !GasTree::exists_and_deposit(GasNodeId::from(new_mid.cast::<PlainNodeId>()))
        {
            return GasTree::split(
                GasNodeId::from(original_mid.cast::<PlainNodeId>()),
                GasNodeId::from(new_mid.cast::<PlainNodeId>()),
            );
        }

        Ok(())
    }

    /// Adapted by argument types version of the gas tree `cut` method.
    pub(crate) fn cut(
        &self,
        original_mid: MessageId,
        new_mid: MessageId,
        amount: Gas,
    ) -> Result<(), GasTreeError> {
        GasTree::cut(
            GasNodeId::from(original_mid.cast::<PlainNodeId>()),
            GasNodeId::from(new_mid.cast::<PlainNodeId>()),
            amount,
        )
    }

    /// Adapted by argument types version of the gas tree `get_limit` method.
    pub(crate) fn get_limit(&self, mid: MessageId) -> Result<Gas, GasTreeError> {
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
    pub(crate) fn consume(&self, mid: MessageId) -> ConsumeResultOf<GasTree> {
        GasTree::consume(GasNodeId::from(mid.cast::<PlainNodeId>()))
    }

    /// Adapted by argument types version of the gas tree `reset` method.
    ///
    /// *Note* Call with caution as it completely resets the storage.
    pub(crate) fn reset(&self) {
        <AuxiliaryGasProvider as Provider>::reset();
    }

    /// Adapted by argument types version of the gas tree `reset` method.
    ///
    /// *Note* Call with caution as it completely resets the storage.
    pub(crate) fn reset(&self) {
        <AuxiliaryGasProvider as Provider>::reset();
    }
}
