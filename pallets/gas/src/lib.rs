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

#![cfg_attr(not(feature = "std"), no_std)]

use common::{
    storage::{MapStorage, ValueStorage},
    Origin,
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

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The maximum amount of gas that can be used within a single block.
        #[pallet::constant]
        type BlockGasLimit: Get<u64>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    // Gas pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Forbidden operation for the value node
        Forbidden,

        /// Gas (gas tree) has already been created for the provided key.
        NodeAlreadyExists,

        /// Account doesn't have enough funds to complete operation.
        InsufficientBalance,

        /// Value node doesn't exist for a key
        NodeNotFound,

        /// Creating node with existing id
        KeyAlreadyExists,

        /// Procedure can't be called on consumed node
        NodeWasConsumed,

        /// Errors stating that gas tree has been invalidated

        /// Parent must be in the tree, but not found
        ///
        /// This differs from `Error::<T>::NodeNotFound`, because parent
        /// node for local node types must be found, but was not. Thus,
        /// tree is invalidated.
        ParentIsLost,

        /// Parent node must have children, but they weren't found
        ///
        /// If node is a parent to some other node it must have at least
        /// one child, otherwise it's id can't be used as a parent for
        /// local nodes in the tree.
        ParentHasNoChildren,
    }

    impl<T: Config> common::value_tree::Error for Error<T> {
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
    }

    pub type Balance = u64;

    // ----

    #[pallet::storage]
    pub type TotalIssuance<T> = StorageValue<_, Balance>;

    common::wrap_storage_value!(
        storage: TotalIssuance,
        name: TotalIssuanceWrap,
        value: Balance
    );

    // ----

    pub type Key = H256;
    pub type ExternalOrigin = H256;
    pub type ValueNode = common::value_tree::ValueNode<ExternalOrigin, Key, Balance>;

    #[pallet::storage]
    pub type ValueTreeNodes<T> = StorageMap<_, Identity, H256, ValueNode>;

    common::wrap_storage_map!(
        storage: ValueTreeNodes,
        name: ValueTreeNodesWrap,
        key: Key,
        value: ValueNode
    );

    // ----

    #[pallet::storage]
    #[pallet::getter(fn gas_allowance)]
    pub type Allowance<T> = StorageValue<_, u64, ValueQuery, BlockGasLimitOf<T>>;

    impl<T: Config> common::ValueTreeProvider for Pallet<T> {
        type BlockGasLimit = BlockGasLimitOf<T>;
        type ExternalOrigin = ExternalOrigin;
        type Key = Key;
        type Balance = Balance;
        type PositiveImbalance =
            common::value_tree::PositiveImbalance<Self::Balance, TotalIssuanceWrap<T>>;
        type NegativeImbalance =
            common::value_tree::NegativeImbalance<Self::Balance, TotalIssuanceWrap<T>>;
        type InternalError = Error<T>;
        type Error = DispatchError;

        type ValueTree = common::value_tree::ValueTreeImpl<
            TotalIssuanceWrap<T>,
            Self::InternalError,
            Self::Error,
            ExternalOrigin,
            ValueTreeNodesWrap<T>,
        >;
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

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        pub fn update_gas_allowance(gas: u64) {
            Allowance::<T>::put(gas);
        }

        pub fn decrease_gas_allowance(gas: u64) {
            Allowance::<T>::mutate(|v| *v = v.saturating_sub(gas));
        }
    }
}
