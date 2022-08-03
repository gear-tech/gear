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
use frame_support::{
    traits::{tokens::Balance as BalanceTrait, Imbalance, SameOrOther, TryDrop},
    RuntimeDebug,
};
use sp_runtime::traits::Zero;
use sp_std::{marker::PhantomData, mem};

mod error;
mod internal;
mod negative_imbalance;
mod node;
mod positive_imbalance;

#[cfg(test)]
mod property_tests;

pub use error::Error;
pub use internal::TreeImpl;
pub use negative_imbalance::NegativeImbalance;
pub use node::GasNode;
pub use positive_imbalance::PositiveImbalance;

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

    /// Type that identifies a particular value item.
    type Key;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is
    /// operations that create inequality between the underlying value
    /// supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been created,
    /// which will eventually lead to an increase in total supply.
    type PositiveImbalance: Imbalance<Self::Balance, Opposite = Self::NegativeImbalance>;

    /// `NegativeImbalance` indicates that some value has been removed
    /// from circulation leading to a decrease in the total supply
    /// of the underlying value.
    type NegativeImbalance: Imbalance<Self::Balance, Opposite = Self::PositiveImbalance>;

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
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, Self::Error>;

    /// The id of node and external origin for a key.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn get_origin_node(key: Self::Key) -> Result<(Self::ExternalOrigin, Self::Key), Self::Error>;

    /// The external origin for a key.
    ///
    /// See [`get_origin_node`](Self::get_origin_node) for details.
    fn get_external(key: Self::Key) -> Result<Self::ExternalOrigin, Self::Error> {
        Self::get_origin_node(key).map(|(external, _key)| external)
    }

    /// The id of external node for a key.
    ///
    /// See [`get_origin_node`](Self::get_origin_node) for details.
    fn get_origin_key(key: Self::Key) -> Result<Self::Key, Self::Error> {
        Self::get_origin_node(key).map(|(_external, key)| key)
    }

    /// Get value associated with given id and the key of an ancestor,
    /// that keeps this value.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the
    /// node identified by the `key` belongs to a subtree originating at
    /// such "orphan" node, or in case of inexistent key.
    fn get_limit_node(key: Self::Key) -> Result<(Self::Balance, Self::Key), Self::Error>;

    /// Get value associated with given id.
    ///
    /// See [`get_limit_node`](Self::get_limit_node) for details.
    fn get_limit(key: Self::Key) -> Result<Self::Balance, Self::Error> {
        Self::get_limit_node(key).map(|(balance, _key)| balance)
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
    fn consume(key: Self::Key) -> ConsumeResultOf<Self>;

    /// Burn underlying value.
    ///
    /// This "spends" the specified amount of value thereby decreasing the
    /// overall supply of it. In case of a success, this indicates the
    /// entire value supply becomes over-collateralized,
    /// hence negative imbalance.
    fn spend(key: Self::Key, amount: Self::Balance)
        -> Result<Self::NegativeImbalance, Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn split_with_value(
        key: Self::Key,
        new_key: Self::Key,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn split(key: Self::Key, new_key: Self::Key) -> Result<(), Self::Error>;

    /// Cut underlying value to a reserved node.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's
    /// locked under that key, an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn cut(key: Self::Key, new_key: Self::Key, amount: Self::Balance) -> Result<(), Self::Error>;

    /// Removes all values.
    fn clear();
}

/// Represents logic of centralized GasTree-algorithm.
pub trait Provider {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a particular value item.
    type Key;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is
    /// operations that create inequality between the underlying value
    /// supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been created,
    /// which will eventually lead to an increase in total supply.
    type PositiveImbalance: Imbalance<Self::Balance, Opposite = Self::NegativeImbalance>;

    /// `NegativeImbalance` indicates that some value has been removed from
    /// circulation leading to a decrease in the total supply of the
    /// underlying value.
    type NegativeImbalance: Imbalance<Self::Balance, Opposite = Self::PositiveImbalance>;

    type InternalError: Error;

    /// Error type.
    type Error: From<Self::InternalError>;

    /// A ledger to account for gas creation and consumption.
    type GasTree: Tree<
        ExternalOrigin = Self::ExternalOrigin,
        Key = Self::Key,
        Balance = Self::Balance,
        PositiveImbalance = Self::PositiveImbalance,
        NegativeImbalance = Self::NegativeImbalance,
        InternalError = Self::InternalError,
        Error = Self::Error,
    >;

    /// Resets all related to gas provider storages.
    ///
    /// It's a temporary production solution to avoid DB migrations
    /// and would be available for test purposes only in the future.
    fn reset() {
        Self::GasTree::clear();
    }
}
