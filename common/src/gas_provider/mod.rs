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

#[cfg(all(test, not(feature = "std")))]
mod property_tests;

pub use error::Error;
pub use internal::TreeImpl;
pub use negative_imbalance::NegativeImbalance;
pub use node::{GasNode, GasNodeType};
pub use positive_imbalance::PositiveImbalance;

/// Abstraction for a chain of value items each piece of which has an attributed owner and
/// can be traced up to some root origin.
/// The definition is largely inspired by the `frame_support::traits::Currency` -
/// <https://github.com/paritytech/substrate/blob/master/frame/support/src/traits/tokens/currency.rs>,
/// however, the intended use is very close to the UTxO based ledger model.
pub trait Tree {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a particular value item.
    type Key;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is operations that create
    /// inequality between the underlying value supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been created, which will eventually
    /// lead to an increase in total supply.
    type PositiveImbalance: Imbalance<Self::Balance, Opposite = Self::NegativeImbalance>;

    /// `NegativeImbalance` indicates that some value has been removed from circulation
    /// leading to a decrease in the total supply of the underlying value.
    type NegativeImbalance: Imbalance<Self::Balance, Opposite = Self::PositiveImbalance>;

    type InternalError: Error;

    /// Error type
    type Error: From<Self::InternalError>;

    /// The total amount of value currently in circulation.
    fn total_supply() -> Self::Balance;

    /// Increase the total issuance of the underlying value by creating some `amount` of it
    /// and attributing it to the `origin`. The `key` identifies the created "bag" of value.
    /// In case the `key` already identifies some other piece of value an error is returned.
    fn create(
        origin: Self::ExternalOrigin,
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, Self::Error>;

    /// The id of node and external origin for a key, if they exist, `None` otherwise.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the node identified by
    /// the `key` belongs to a subtree originating at such "orphan" node.
    fn get_origin(
        key: Self::Key,
    ) -> Result<OriginResult<Self::Key, Self::ExternalOrigin>, Self::Error>;

    /// The external origin for a key, if the latter exists, `None` otherwise.
    ///
    /// Check [`get_origin`](Self::get_origin) for more details.
    fn get_external(key: Self::Key) -> Result<Option<Self::ExternalOrigin>, Self::Error> {
        Self::get_origin(key).map(|result| result.map(|(_, external)| external))
    }

    /// The id of external node for a key, if the latter exists, `None` otherwise.
    ///
    /// Check [`get_origin`](Self::get_origin) for more details.
    fn get_origin_key(key: Self::Key) -> Result<Option<Self::Key>, Self::Error> {
        Self::get_origin(key).map(|result| result.map(|(key, _)| key))
    }

    /// Get value item by it's ID, if exists, and the key of an ancestor that sets this limit.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the node identified by
    /// the `key` belongs to a subtree originating at such "orphan" node.
    fn get_limit(key: Self::Key) -> Result<GasBalanceKey<Self::Balance, Self::Key>, Self::Error>;

    /// Consume underlying value.
    ///
    /// If `key` does not identify any value or the value can't be fully consumed due to
    /// being a part of other value or itself having unconsumed parts, return `None`,
    /// else the corresponding piece of value is destroyed and imbalance is created.
    ///
    /// Error occurs if the tree is invalidated (has "orphan" nodes), and the node identified by
    /// the `key` belongs to a subtree originating at such "orphan" node.
    fn consume(
        key: Self::Key,
    ) -> Result<ConsumeOutput<Self::NegativeImbalance, Self::ExternalOrigin>, Self::Error>;

    /// Burns underlying value.
    ///
    /// This "spends" the specified amount of value thereby decreasing the overall supply of it.
    /// In case of a success, this indicates the entire value supply becomes over-collateralized,
    /// hence negative imbalance.
    fn spend(key: Self::Key, amount: Self::Balance)
        -> Result<Self::NegativeImbalance, Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's locked under that key,
    /// an error is returned.
    /// This can't create imbalance as no value is burned or created.
    fn split_with_value(
        key: Self::Key,
        new_key: Self::Key,
        amount: Self::Balance,
    ) -> Result<(), Self::Error>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value an error is returned.
    /// This can't create imbalance as no value is burned or created.
    fn split(key: Self::Key, new_key: Self::Key) -> Result<(), Self::Error>;

    /// Cut underlying value to a reserved node.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's locked under that key,
    /// an error is returned.
    ///
    /// This can't create imbalance as no value is burned or created.
    fn cut(key: Self::Key, new_key: Self::Key, amount: Self::Balance) -> Result<(), Self::Error>;
}

pub type GasBalanceKey<Balance, Key> = Option<(Balance, Key)>;
pub type OriginResult<Key, ExternalOrigin> = Option<(Key, ExternalOrigin)>;
pub type ConsumeOutput<Imbalance, External> = Option<(Imbalance, External)>;

/// Represents logic of centralized GasTree-algorithm.
pub trait Provider {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a particular value item.
    type Key;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is operations that create
    /// inequality between the underlying value supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been created, which will eventually
    /// lead to an increase in total supply.
    type PositiveImbalance: Imbalance<Self::Balance, Opposite = Self::NegativeImbalance>;

    /// `NegativeImbalance` indicates that some value has been removed from circulation
    /// leading to a decrease in the total supply of the underlying value.
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
}
