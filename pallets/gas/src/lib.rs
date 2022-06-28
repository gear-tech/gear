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
    GasProvider,
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
        Forbidden,
        NodeAlreadyExists,
        InsufficientBalance,
        NodeNotFound,
        NodeWasConsumed,
        ParentIsLost,
        ParentHasNoChildren,
    }

    impl<T: Config> common::gas_provider::Error for Error<T> {
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
    pub type ValueNode = common::gas_provider::GasNode<ExternalOrigin, Key, Balance>;

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
        type BlockGasLimit = BlockGasLimitOf<T>;
        type ExternalOrigin = ExternalOrigin;
        type Key = Key;
        type Balance = Balance;
        type PositiveImbalance =
            common::gas_provider::PositiveImbalance<Self::Balance, TotalIssuanceWrap<T>>;
        type NegativeImbalance =
            common::gas_provider::NegativeImbalance<Self::Balance, TotalIssuanceWrap<T>>;
        type InternalError = Error<T>;
        type Error = DispatchError;

        type GasTree = common::gas_provider::TreeImpl<
            TotalIssuanceWrap<T>,
            Self::InternalError,
            Self::Error,
            ExternalOrigin,
            ValueTreeNodesWrap<T>,
        >;

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
