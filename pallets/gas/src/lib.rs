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

pub use self::imbalances::{NegativeImbalance, PositiveImbalance};
use common::{Origin, ValueTree};
use frame_support::{dispatch::DispatchError, pallet_prelude::*, traits::Imbalance};
pub use pallet::*;
pub use primitive_types::H256;
use scale_info::TypeInfo;
use sp_std::convert::TryInto;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub enum ValueType {
    External { id: H256, value: u64 },
    SpecifiedLocal { parent: H256, value: u64 },
    UnspecifiedLocal { parent: H256 },
}

impl Default for ValueType {
    fn default() -> Self {
        ValueType::External {
            id: H256::default(),
            value: 0,
        }
    }
}

#[derive(Clone, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub struct ValueNode {
    pub id: H256,
    pub spec_refs: u32,
    pub unspec_refs: u32,
    pub inner: ValueType,
    pub consumed: bool,
}

impl ValueNode {
    pub fn new(origin: H256, id: H256, value: u64) -> Self {
        Self {
            id,
            inner: ValueType::External { id: origin, value },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        }
    }

    pub fn inner_value(&self) -> Option<u64> {
        match self.inner {
            ValueType::External { value, .. } => Some(value),
            ValueType::SpecifiedLocal { value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn inner_value_mut(&mut self) -> Option<&mut u64> {
        match self.inner {
            ValueType::External { ref mut value, .. } => Some(value),
            ValueType::SpecifiedLocal { ref mut value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn parent(&self) -> Option<H256> {
        match self.inner {
            ValueType::External { .. } => None,
            ValueType::SpecifiedLocal { parent, .. } => Some(parent),
            ValueType::UnspecifiedLocal { parent } => Some(parent),
        }
    }

    pub fn refs(&self) -> u32 {
        self.spec_refs.saturating_add(self.unspec_refs)
    }

    /// The first upstream node (self included), that holds a concrete value.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    pub fn node_with_value<T: Config>(&self) -> Result<ValueNode, DispatchError> {
        if let ValueType::UnspecifiedLocal { parent } = self.inner {
            <Pallet<T>>::get_node(parent)
                .ok_or(Error::<T>::GasTreeInvalidated)?
                .node_with_value::<T>()
        } else {
            Ok(self.clone())
        }
    }

    /// Returns the AccountId (as Origin) of the value tree creator.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    pub fn root_origin<T: Config>(&self) -> Result<H256, DispatchError> {
        match self.inner {
            ValueType::External { id, .. } => Ok(id),
            ValueType::SpecifiedLocal { parent, .. } | ValueType::UnspecifiedLocal { parent } => {
                <Pallet<T>>::get_node(parent)
                    .ok_or(<Error<T>>::GasTreeInvalidated)?
                    .root_origin::<T>()
            }
        }
    }
}

pub type ConsumeOutput<T> = Option<(NegativeImbalance<T>, H256)>;

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
        /// Gas (gas tree) has already been created for the provided key.
        GasTreeAlreadyExists,

        /// Account doesn't have enough funds to complete operation.
        InsufficientBalance,

        /// Value node doesn't exist for a key
        NodeNotFound,

        /// Gas tree has been invalidated
        GasTreeInvalidated,
    }

    #[pallet::storage]
    #[pallet::getter(fn gas_allowance)]
    pub type Allowance<T> = StorageValue<_, u64, ValueQuery, <T as Config>::BlockGasLimit>;

    #[pallet::storage]
    #[pallet::getter(fn total_issuance)]
    pub type TotalIssuance<T> = StorageValue<_, u64, ValueQuery, GetDefault>;

    #[pallet::storage]
    #[pallet::getter(fn get_node)]
    pub type GasTree<T> = StorageMap<_, Identity, H256, ValueNode>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Reset block gas allowance
            Allowance::<T>::put(T::BlockGasLimit::get());

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

        /// Check if a node is consumed and does not have any child nodes so it can be deleted.
        /// If the node's type is `ValueType::External`, the locked value is released to the owner.
        /// Otherwise this function is called for the parent node to propagate the process furter.
        pub(super) fn check_consumed(key: H256) -> Result<ConsumeOutput<T>, DispatchError> {
            let mut delete_current_node = false;
            let maybe_node = Self::get_node(key);
            let outcome = if let Some(current_node) = maybe_node {
                match current_node.inner {
                    ValueType::SpecifiedLocal { parent, .. }
                    | ValueType::UnspecifiedLocal { parent } => {
                        if current_node.consumed && current_node.refs() == 0 {
                            // Parent node must exist for any node; if it doesn't, the tree's become invalid
                            let mut parent_node =
                                Self::get_node(parent).ok_or(Error::<T>::GasTreeInvalidated)?;
                            assert!(
                                parent_node.refs() > 0,
                                "parent node must contain ref to its child node"
                            );

                            if let ValueType::SpecifiedLocal { .. } = current_node.inner {
                                parent_node.spec_refs = parent_node.spec_refs.saturating_sub(1);
                            } else {
                                parent_node.unspec_refs = parent_node.unspec_refs.saturating_sub(1);
                            }

                            GasTree::<T>::mutate(parent, |node| {
                                *node = Some(parent_node);
                            });

                            if let ValueType::SpecifiedLocal {
                                value: self_value, ..
                            } = current_node.inner
                            {
                                // this is specified, so it needs to get to the first specified parent also
                                // going up until external or specified parent is found

                                // `parent` key is known to exist, hence there must be a node.
                                // If there isn't, the gas tree is considered corrupted (invalidated).
                                let mut parent_node = Self::get_node(parent)
                                    .ok_or(Error::<T>::GasTreeInvalidated)?
                                    // a node with value must exist for a node, unless tree corrupted
                                    .node_with_value::<T>()?;

                                // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
                                let parent_val = parent_node
                                    .inner_value_mut()
                                    .expect("Querying parent with value");

                                *parent_val = parent_val.saturating_add(self_value);

                                GasTree::<T>::mutate(parent_node.id, |value| {
                                    *value = Some(parent_node);
                                });
                            }

                            delete_current_node = true;
                            Self::check_consumed(parent)?
                        } else {
                            None
                        }
                    }
                    ValueType::External { id, value } => {
                        if current_node.refs() == 0 && current_node.consumed {
                            delete_current_node = true;
                            Some((NegativeImbalance::new(value), id))
                        } else {
                            None
                        }
                    }
                }
            } else {
                None
            };

            if delete_current_node {
                GasTree::<T>::remove(key);
            }

            Ok(outcome)
        }
    }
}

impl<T: Config> ValueTree for Pallet<T>
where
    T::AccountId: Origin,
{
    type ExternalOrigin = H256;
    type Key = H256;
    type Balance = u64;
    type PositiveImbalance = PositiveImbalance<T>;
    type NegativeImbalance = NegativeImbalance<T>;
    type Error = DispatchError;

    fn total_supply() -> u64 {
        Self::total_issuance()
    }

    /// Releases in circulation a certain amount of newly created gas
    fn create(origin: H256, key: H256, amount: u64) -> Result<PositiveImbalance<T>, DispatchError> {
        ensure!(
            !GasTree::<T>::contains_key(key),
            Error::<T>::GasTreeAlreadyExists
        );

        let node = ValueNode::new(origin, key, amount);

        // Save value node to storage
        GasTree::<T>::insert(key, node);

        Ok(PositiveImbalance::new(amount))
    }

    fn get_origin(key: H256) -> Result<Option<H256>, DispatchError> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            Some(node.root_origin::<T>()?)
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_limit(key: H256) -> Result<Option<u64>, DispatchError> {
        if let Some(node) = Self::get_node(key) {
            Ok({
                let node_with_value = node.node_with_value::<T>()?;
                // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
                let v = node_with_value
                    .inner_value()
                    .expect("The node here is either external or specified, hence the inner value");
                Some(v)
            })
        } else {
            Ok(None)
        }
    }

    fn consume(key: H256) -> Result<ConsumeOutput<T>, DispatchError> {
        let mut delete_current_node = false;
        let mut consume_parent_node = false;
        let maybe_node = Self::get_node(key);
        let outcome = if let Some(mut node) = maybe_node {
            match node.inner {
                ValueType::UnspecifiedLocal { parent }
                | ValueType::SpecifiedLocal { parent, .. } => {
                    // Parent node must exist for any node; if it doesn't, the tree's become invalid
                    let mut parent_node =
                        Self::get_node(parent).ok_or(Error::<T>::GasTreeInvalidated)?;
                    assert!(
                        parent_node.refs() > 0,
                        "parent node must contain ref to its child node"
                    );

                    if node.refs() == 0 {
                        delete_current_node = true;

                        if let ValueType::SpecifiedLocal { .. } = node.inner {
                            parent_node.spec_refs = parent_node.spec_refs.saturating_sub(1);
                        } else {
                            parent_node.unspec_refs = parent_node.unspec_refs.saturating_sub(1);
                        }

                        if parent_node.refs() == 0 {
                            consume_parent_node = true;
                        }

                        // Update parent node
                        GasTree::<T>::mutate(parent, |value| {
                            *value = Some(parent_node);
                        });
                    }

                    if let ValueType::SpecifiedLocal {
                        value: self_value, ..
                    } = node.inner
                    {
                        if node.unspec_refs == 0 {
                            // Any node of type `ValueType::SpecifiedLocal` must have a parent
                            let mut parent_node = Self::get_node(parent)
                                .ok_or(<Error<T>>::GasTreeInvalidated)?
                                // Upstream node with a concrete value must exist for any node
                                .node_with_value::<T>()?;

                            // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
                            let parent_val = parent_node
                                .inner_value_mut()
                                .expect("Querying parent with value");
                            *parent_val = parent_val.saturating_add(self_value);

                            GasTree::<T>::mutate(parent_node.id, |value| {
                                *value = Some(parent_node);
                            });
                        }
                    }

                    if delete_current_node {
                        GasTree::<T>::remove(key);
                    } else {
                        node.consumed = true;

                        if node.unspec_refs == 0 {
                            if let Some(inner_value) = node.inner_value_mut() {
                                *inner_value = 0
                            };
                        }
                        // Save current node
                        GasTree::<T>::mutate(key, |value| {
                            *value = Some(node);
                        });
                    }

                    // now check if the parent node can be consumed as well
                    if consume_parent_node {
                        Self::check_consumed(parent)?
                    } else {
                        None
                    }
                }
                ValueType::External { id, value } => {
                    node.consumed = true;

                    if node.refs() == 0 {
                        // Delete current node
                        GasTree::<T>::remove(key);

                        Some((NegativeImbalance::new(value), id))
                    } else {
                        // Save current node
                        GasTree::<T>::mutate(key, |n| {
                            *n = Some(node);
                        });

                        None
                    }
                }
            }
        } else {
            None
        };

        Ok(outcome)
    }

    fn spend(key: H256, amount: u64) -> Result<NegativeImbalance<T>, DispatchError> {
        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let mut node = Self::get_node(key)
            .ok_or(Error::<T>::NodeNotFound)?
            .node_with_value::<T>()?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        ensure!(
            node.inner_value().expect("Querying node with value") >= amount,
            Error::<T>::InsufficientBalance
        );
        *node.inner_value_mut().expect("Querying node with value") -= amount;
        log::debug!("Spent {} of gas", amount);

        // Save node that deliveres limit
        GasTree::<T>::mutate(node.id, |value| {
            *value = Some(node);
        });

        Ok(NegativeImbalance::new(amount))
    }

    fn split(key: H256, new_node_key: H256) -> DispatchResult {
        let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        node.unspec_refs = node.unspec_refs.saturating_add(1);

        let new_node = ValueNode {
            id: new_node_key,
            inner: ValueType::UnspecifiedLocal { parent: key },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        GasTree::<T>::insert(new_node_key, new_node);
        // Update current node
        GasTree::<T>::mutate(key, |value| {
            *value = Some(node);
        });

        Ok(())
    }

    fn split_with_value(key: H256, new_node_key: H256, amount: u64) -> DispatchResult {
        let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let node_with_value = node.node_with_value::<T>()?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        ensure!(
            node_with_value
                .inner_value()
                .expect("Querying node with value")
                >= amount,
            Error::<T>::InsufficientBalance
        );

        node.spec_refs = node.spec_refs.saturating_add(1);

        let new_node = ValueNode {
            id: new_node_key,
            inner: ValueType::SpecifiedLocal {
                value: amount,
                parent: key,
            },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        GasTree::<T>::insert(new_node_key, new_node);
        // Update current node
        GasTree::<T>::mutate(key, |value| {
            *value = Some(node);
        });

        // re-querying it since it might be the same node we already updated above.. :(
        let mut node_with_value =
            // NOTE: intentional expects. Querying the same nodes we did earlier in this function
            Self::get_node(key).expect("Node exists")
                .node_with_value::<T>().expect("Node with value exists");

        // NOTE: intentional expects. A node_with_value is guaranteed to have inner_value
        ensure!(
            node_with_value
                .inner_value()
                .expect("Querying node with value")
                >= amount,
            Error::<T>::InsufficientBalance
        );
        *node_with_value
            .inner_value_mut()
            .expect("Querying node with value") -= amount;
        GasTree::<T>::mutate(node_with_value.id, |value| {
            *value = Some(node_with_value);
        });

        Ok(())
    }
}

// Wrapping the imbalances in a private module to ensure privacy of the inner members.
mod imbalances {
    use super::{Config, Imbalance};
    use frame_support::{
        traits::{SameOrOther, TryDrop},
        RuntimeDebug,
    };
    use sp_runtime::traits::Zero;
    use sp_std::{marker::PhantomData, mem};

    /// Opaque, move-only struct with private field to denote that value has been created
    /// without any equal and opposite accounting
    #[derive(RuntimeDebug, PartialEq, Eq)]
    pub struct PositiveImbalance<T: Config>(u64, PhantomData<T>);

    impl<T: Config> PositiveImbalance<T> {
        /// Create a new positive imbalance from value amount.
        pub fn new(amount: u64) -> Self {
            PositiveImbalance(amount, PhantomData)
        }
    }

    /// Opaque, move-only struct with private field to denote that value has been destroyed
    /// without any equal and opposite accounting.
    #[derive(RuntimeDebug, PartialEq, Eq)]
    pub struct NegativeImbalance<T: Config>(u64, PhantomData<T>);

    impl<T: Config> NegativeImbalance<T> {
        /// Create a new negative imbalance from value amount.
        pub fn new(amount: u64) -> Self {
            NegativeImbalance(amount, PhantomData)
        }
    }

    impl<T: Config> TryDrop for PositiveImbalance<T> {
        fn try_drop(self) -> Result<(), Self> {
            self.drop_zero()
        }
    }

    impl<T: Config> Default for PositiveImbalance<T> {
        fn default() -> Self {
            Self::zero()
        }
    }

    impl<T: Config> Imbalance<u64> for PositiveImbalance<T> {
        type Opposite = NegativeImbalance<T>;

        fn zero() -> Self {
            Self(Zero::zero(), PhantomData)
        }

        fn drop_zero(self) -> Result<(), Self> {
            if self.0.is_zero() {
                Ok(())
            } else {
                Err(self)
            }
        }

        fn split(self, amount: u64) -> (Self, Self) {
            let first = self.0.min(amount);
            let second = self.0 - first;

            mem::forget(self);
            (Self(first, PhantomData), Self(second, PhantomData))
        }

        fn merge(mut self, other: Self) -> Self {
            self.0 = self.0.saturating_add(other.0);
            mem::forget(other);

            self
        }

        fn subsume(&mut self, other: Self) {
            self.0 = self.0.saturating_add(other.0);
            mem::forget(other);
        }

        #[allow(clippy::comparison_chain)]
        fn offset(self, other: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
            let (a, b) = (self.0, other.0);
            mem::forget((self, other));

            if a > b {
                SameOrOther::Same(Self(a - b, PhantomData))
            } else if b > a {
                SameOrOther::Other(NegativeImbalance::new(b - a))
            } else {
                SameOrOther::None
            }
        }

        fn peek(&self) -> u64 {
            self.0
        }
    }

    impl<T: Config> TryDrop for NegativeImbalance<T> {
        fn try_drop(self) -> Result<(), Self> {
            self.drop_zero()
        }
    }

    impl<T: Config> Default for NegativeImbalance<T> {
        fn default() -> Self {
            Self::zero()
        }
    }

    impl<T: Config> Imbalance<u64> for NegativeImbalance<T> {
        type Opposite = PositiveImbalance<T>;

        fn zero() -> Self {
            Self(Zero::zero(), PhantomData)
        }

        fn drop_zero(self) -> Result<(), Self> {
            if self.0.is_zero() {
                Ok(())
            } else {
                Err(self)
            }
        }

        fn split(self, amount: u64) -> (Self, Self) {
            let first = self.0.min(amount);
            let second = self.0 - first;

            mem::forget(self);
            (Self(first, PhantomData), Self(second, PhantomData))
        }

        fn merge(mut self, other: Self) -> Self {
            self.0 = self.0.saturating_add(other.0);
            mem::forget(other);

            self
        }

        fn subsume(&mut self, other: Self) {
            self.0 = self.0.saturating_add(other.0);
            mem::forget(other);
        }

        #[allow(clippy::comparison_chain)]
        fn offset(self, other: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
            let (a, b) = (self.0, other.0);
            mem::forget((self, other));

            if a > b {
                SameOrOther::Same(Self(a - b, PhantomData))
            } else if b > a {
                SameOrOther::Other(PositiveImbalance::new(b - a))
            } else {
                SameOrOther::None
            }
        }

        fn peek(&self) -> u64 {
            self.0
        }
    }

    impl<T: Config> Drop for PositiveImbalance<T> {
        /// Basic drop handler will just square up the total issuance.
        fn drop(&mut self) {
            <super::TotalIssuance<T>>::mutate(|v| *v = v.saturating_add(self.0));
        }
    }

    impl<T: Config> Drop for NegativeImbalance<T> {
        /// Basic drop handler will just square up the total issuance.
        fn drop(&mut self) {
            <super::TotalIssuance<T>>::mutate(|v| {
                if self.0 > *v {
                    log::debug!(
                        target: "essential",
                        "Unaccounted gas detected: burnt {:?}, known total supply was {:?}.",
                        self.0,
                        *v
                    )
                }
                *v = v.saturating_sub(self.0)
            });
        }
    }
}
