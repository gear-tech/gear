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

//! # Gear Gas Pallet
//!
//! The Gear Gas Pallet provides functionality for handling messages'
//! execution resources.
//!
//! - [`Config`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! The Gear Gas Pallet's main aim is to separate message's associated gas tree nodes storages out
//! of Gear's execution logic and provide soft functionality to manage them.
//!
//! The Gear Gas Pallet provides functions for:
//! - Obtaining maximum gas amount available within one block of execution.
//! - Managing number of remaining gas, i.e. gas allowance.
//! - Managing gas tree: create, split, cut, etc new nodes determining
//! execution resources of messages.
//!
//! ## Interface
//!
//! The Gear Gas Pallet implements `gear_common::GasProvider` trait
//! and shouldn't contain any other functionality, except this trait declares.
//!
//! ## Usage
//!
//! How to use the gas functionality from the Gear Gas Pallet:
//!
//! 1. Implement its `Config` for your runtime with specified `BlockGasLimit` type.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl pallet_gear_gas::Config for Runtime {
//!     type BlockGasLimit = .. ;
//! }
//!
//! // ... //
//! ```
//!
//! 2. Provide associated type for your pallet's `Config`, which implements
//! `gear_common::GasProvider` trait, specifying associated types if needed.
//!
//! ```ignore
//! // `some_pallet/src/lib.rs`
//! // ... //
//!
//! use gear_common::GasProvider;
//!
//! #[pallet::config]
//! pub trait Config: frame_system::Config {
//!     // .. //
//!
//!     type GasProvider: GasProvider<Balance = u64, ...>;
//!
//!     // .. //
//! }
//! ```
//!
//! 3. Declare Gear Gas Pallet in your `construct_runtime!` macro.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! construct_runtime!(
//!     pub enum Runtime
//!         where // ... //
//!     {
//!         // ... //
//!
//!         GearGas: pallet_gear_gas,
//!
//!         // ... //
//!     }
//! );
//!
//! // ... //
//! ```
//! `GearGas: pallet_gear_gas,`
//!
//! 4. Set `GearGas` as your pallet `Config`'s `GasProvider` type.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl some_pallet::Config for Runtime {
//!     // ... //
//!
//!     type GasProvider = GearGas;
//!
//!     // ... //
//! }
//!
//! // ... //
//! ```
//!
//! 5. Work with Gear Gas Pallet in your pallet with provided
//! associated type interface.
//!
//! ## Genesis config
//!
//! The Gear Gas Pallet doesn't depend on the `GenesisConfig`.

#![cfg_attr(not(feature = "std"), no_std)]

use common::{
    storage::{MapStorage, ValueStorage},
    BlockLimiter, GasProvider,
};
use frame_support::{dispatch::DispatchError, pallet_prelude::*};
pub use pallet::*;
pub use primitive_types::H256;
use sp_std::convert::TryInto;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

type BlockGasLimitOf<T> = <T as Config>::BlockGasLimit;
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::gas_provider::{Error as GasError, GasNode, GasNodeId, TreeImpl};
    use frame_system::pallet_prelude::*;
    use gear_core::ids::{MessageId, ReservationId};

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The maximum amount of gas that can be used within a single block.
        #[pallet::constant]
        type BlockGasLimit: Get<u64>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // Gas pallet error.
    #[pallet::error]
    pub enum Error<T> {
        Forbidden,
        NodeAlreadyExists,
        InsufficientBalance,
        NodeNotFound,
        NodeWasConsumed,

        /// Errors stating that gas tree has been invalidated
        ParentIsLost,
        ParentHasNoChildren,

        /// Output of `Tree::consume` procedure that wasn't expected.
        ///
        /// Outputs of consumption procedure are determined. The error is returned
        /// when unexpected one occurred. That signals, that algorithm works wrong
        /// and expected invariants are not correct.
        UnexpectedConsumeOutput,

        /// Node type that can't occur if algorithm work well
        UnexpectedNodeType,

        /// Value must have been caught, but was missed or blocked (for more info see `ValueNode::catch_value`).
        ValueIsNotCaught,

        /// Value must have been caught or moved upstream, but was blocked (for more info see `ValueNode::catch_value`).
        ValueIsBlocked,

        /// Value must have been blocked, but was either moved or caught (for more info see `ValueNode::catch_value`).
        ValueIsNotBlocked,

        /// `GasTree::consume` called on node, which has some balance locked.
        ConsumedWithLock,

        /// `GasTree::consume` called on node, which has some system reservation.
        ConsumedWithSystemReservation,

        /// `GasTree::create` called with some value amount leading to
        /// the total value overflow.
        TotalValueIsOverflowed,

        /// Either `GasTree::consume` or `GasTree::spent` called on a node creating
        /// negative imbalance which leads to the total value drop below 0.
        TotalValueIsUnderflowed,
    }

    impl<T: Config> GasError for Error<T> {
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

    pub type Balance = u64;

    // ----

    // Private storage for total issuance of gas.
    #[pallet::storage]
    pub type TotalIssuance<T> = StorageValue<_, Balance>;

    // Public wrap of the total issuance of gas.
    common::wrap_storage_value!(
        storage: TotalIssuance,
        name: TotalIssuanceWrap,
        value: Balance
    );

    // ----

    pub type Key = GasNodeId<MessageId, ReservationId>;
    pub type NodeOf<T> = GasNode<AccountIdOf<T>, Key, Balance>;

    // Private storage for nodes of the gas tree.
    #[pallet::storage]
    #[pallet::unbounded]
    pub type GasNodes<T> = StorageMap<_, Identity, Key, NodeOf<T>>;

    // Public wrap of the nodes of the gas tree.
    common::wrap_storage_map!(
        storage: GasNodes,
        name: GasNodesWrap,
        key: Key,
        value: NodeOf<T>
    );

    // ----

    #[pallet::storage]
    pub type Allowance<T> = StorageValue<_, Balance, ValueQuery, BlockGasLimitOf<T>>;

    pub struct GasAllowance<T: Config>(PhantomData<T>);

    impl<T: Config> common::storage::Limiter for GasAllowance<T> {
        type Value = Balance;

        fn get() -> Self::Value {
            Allowance::<T>::get()
        }

        fn put(gas: Self::Value) {
            Allowance::<T>::put(gas);
        }

        fn decrease(gas: Self::Value) {
            Allowance::<T>::mutate(|v| *v = v.saturating_sub(gas));
        }
    }

    impl<T: Config> GasProvider for Pallet<T> {
        type ExternalOrigin = AccountIdOf<T>;
        type Key = MessageId;
        type ReservationKey = ReservationId;
        type Balance = Balance;
        type InternalError = Error<T>;
        type Error = DispatchError;

        type GasTree = TreeImpl<
            TotalIssuanceWrap<T>,
            Self::InternalError,
            Self::Error,
            Self::ExternalOrigin,
            GasNodesWrap<T>,
        >;
    }

    impl<T: Config> BlockLimiter for Pallet<T> {
        type BlockGasLimit = BlockGasLimitOf<T>;

        type Balance = Balance;

        type GasAllowance = GasAllowance<T>;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Reset block gas allowance
            Allowance::<T>::put(BlockGasLimitOf::<T>::get());

            T::DbWeight::get().writes(1)
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }
}
