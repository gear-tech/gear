// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use frame_support::{
    sp_runtime::traits::Zero, traits::tokens::Balance as BalanceTrait, RuntimeDebug,
};
use sp_std::marker::PhantomData;

mod error;
mod internal;
mod lockable;
mod negative_imbalance;
mod node;
mod positive_imbalance;
mod reservable;

#[cfg(test)]
mod property_tests;

pub use error::Error;
pub use internal::TreeImpl;
pub use lockable::{LockId, LockableTree};
pub use negative_imbalance::NegativeImbalance;
pub use node::{ChildrenRefs, GasNode, GasNodeId, NodeLock};
pub use positive_imbalance::PositiveImbalance;
pub use reservable::ReservableTree;

/// Simplified type for result of `GasTree::consume` call.
pub type ConsumeResultOf<T> = Result<
    Option<(<T as Tree>::NegativeImbalance, <T as Tree>::ExternalOrigin)>,
    <T as Tree>::Error,
>;

/// Abstraction for a chain of value items each piece of which has an attributed
/// owner and can be traced up to some root origin.
///
/// The definition is largely inspired by the `frame_support::traits::Currency`,
/// however, the intended use is very close to the UTxO based ledger model.
pub trait Tree {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a node of the tree.
    type NodeId: Clone;

    /// Type representing a quantity of value.
    type Balance: Clone;

    /// Types to denote a result of some unbalancing operation - that is
    /// operations that create inequality between the underlying value
    /// supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been added
    /// to circulation , i.e. total supply has increased.
    type PositiveImbalance: Imbalance<Balance = Self::Balance>;

    /// `NegativeImbalance` indicates that some value has been removed
    /// from circulation, i.e. total supply has decreased.
    type NegativeImbalance: Imbalance<Balance = Self::Balance>;

    type InternalError: Error;

    /// Error type
    type Error: From<Self::InternalError>;

    /// The total amount of value currently in circulation.
    fn total_supply() -> Self::Balance;

    /// Increase the total issuance of the underlying value by creating some
    /// `amount` of it and attributing it to the `origin`.
    ///
    /// The `key` identifies the created "bag" of value. In case the `key`
    /// already identifies some other piece of value an error is returned.
    fn create(
        origin: Self::ExternalOrigin,
        key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, Self::Error>;

    /// The id of node and external origin for a key.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn get_origin_node(
        key: impl Into<Self::NodeId>,
    ) -> Result<(Self::ExternalOrigin, Self::NodeId), Self::Error>;

    /// The external origin for a key.
    ///
    /// See [`get_origin_node`](Self::get_origin_node) for details.
    fn get_external(key: impl Into<Self::NodeId>) -> Result<Self::ExternalOrigin, Self::Error> {
        Self::get_origin_node(key).map(|(external, _key)| external)
    }

    /// The id of external node for a key.
    ///
    /// See [`get_origin_node`](Self::get_origin_node) for details.
    fn get_origin_key(key: impl Into<Self::NodeId>) -> Result<Self::NodeId, Self::Error> {
        Self::get_origin_node(key).map(|(_external, key)| key)
    }

    /// Get value associated with given id and the key of an ancestor,
    /// that keeps this value.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn get_limit_node(
        key: impl Into<Self::NodeId>,
    ) -> Result<(Self::Balance, Self::NodeId), Self::Error>;

    /// Get value associated with given id and the key of an consumed ancestor,
    /// that keeps this value.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn get_limit_node_consumed(
        key: impl Into<Self::NodeId>,
    ) -> Result<(Self::Balance, Self::NodeId), Self::Error>;

    /// Get value associated with given id.
    ///
    /// See [`get_limit_node`](Self::get_limit_node) for details.
    fn get_limit(key: impl Into<Self::NodeId>) -> Result<Self::Balance, Self::Error> {
        Self::get_limit_node(key).map(|(balance, _key)| balance)
    }

    /// Get value associated with given id within consumed node.
    ///
    /// See [`get_limit_node_consumed`](Self::get_limit_node_consumed) for details.
    fn get_limit_consumed(key: impl Into<Self::NodeId>) -> Result<Self::Balance, Self::Error> {
        Self::get_limit_node_consumed(key).map(|(balance, _key)| balance)
    }

    /// Consume underlying value.
    ///
    /// If `key` does not identify any value or the value can't be fully
    /// consumed due to being a part of other value or itself having
    /// unconsumed parts, return `None`, else the corresponding
    /// piece of value is destroyed and imbalance is created.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn consume(key: impl Into<Self::NodeId>) -> ConsumeResultOf<Self>;

    /// Burn underlying value.
    ///
    /// This "spends" the specified amount of value thereby decreasing the
    /// overall supply of it. In case of a success, this indicates the
    /// entire value supply becomes over-collateralized,
    /// hence negative imbalance.
    fn spend(
        key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<Self::NegativeImbalance, Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn split_with_value(
        key: impl Into<Self::NodeId>,
        new_key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn split(
        key: impl Into<Self::NodeId>,
        new_key: impl Into<Self::NodeId>,
    ) -> Result<(), Self::Error>;

    /// Cut underlying value to a reserved node.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn cut(
        key: impl Into<Self::NodeId>,
        new_key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Creates deposit external node to be used as pre-defined gas node.
    fn create_deposit(
        key: impl Into<Self::NodeId>,
        new_key: impl Into<Self::NodeId>,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Return bool, defining does node exist.
    fn exists(key: impl Into<Self::NodeId>) -> bool;

    /// Returns bool, defining does node exist and is external with deposit.
    fn exists_and_deposit(key: impl Into<Self::NodeId>) -> bool;

    /// Removes all values.
    fn clear();
}

/// Represents logic of centralized GasTree-algorithm.
pub trait Provider {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a tree node.
    type NodeId;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is
    /// operations that create inequality between the underlying value
    /// supply and some hypothetical "collateral" asset.

    type InternalError: Error;

    /// Error type.
    type Error: From<Self::InternalError>;

    /// A ledger to account for gas creation and consumption.
    type GasTree: LockableTree<
            ExternalOrigin = Self::ExternalOrigin,
            NodeId = Self::NodeId,
            Balance = Self::Balance,
            InternalError = Self::InternalError,
            Error = Self::Error,
        > + ReservableTree;

    /// Resets all related to gas provider storages.
    ///
    /// It's a temporary production solution to avoid DB migrations
    /// and would be available for test purposes only in the future.
    fn reset() {
        Self::GasTree::clear();
    }
}

/// Represents either added or removed value to/from total supply of the currency.
pub trait Imbalance {
    type Balance;

    /// Returns imbalance raw value.
    fn peek(&self) -> Self::Balance;

    /// Applies imbalance to some amount.
    fn apply_to(&self, amount: &mut Option<Self::Balance>) -> Result<(), ImbalanceError>;
}

/// Represents errors returned by via the [Imbalance] trait.
/// Indicates the imbalance value causes amount value overflowing
/// when applied to the latter.
#[derive(Debug, PartialEq)]
pub struct ImbalanceError;
