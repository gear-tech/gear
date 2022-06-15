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
mod property_tests;
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

    /// The first upstream node (self included), that is able to hold a concrete value, but doesn't
    /// necessarily has a non-zero value.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    pub fn node_with_value<T: Config>(&self) -> Result<ValueNode, DispatchError> {
        let mut ret_node = self.clone();
        while let ValueType::UnspecifiedLocal { parent } = ret_node.inner {
            ret_node = <Pallet<T>>::get_node(parent).ok_or(Error::<T>::GasTreeInvalidated)?;
        }

        Ok(ret_node)
    }

    /// Returns the AccountId (as Origin) of the value tree creator.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    pub fn root<T: Config>(&self) -> Result<Self, DispatchError> {
        let mut ret_node = self.clone();
        while let Some(parent) = ret_node.parent() {
            ret_node = <Pallet<T>>::get_node(parent).ok_or(Error::<T>::GasTreeInvalidated)?;
        }
        Ok(ret_node)
    }

    fn decrease_parents_ref<T: Config>(&self) -> DispatchResult {
        if let Some(id) = self.parent() {
            let mut parent = <Pallet<T>>::get_node(id).ok_or(Error::<T>::GasTreeInvalidated)?;
            assert!(
                parent.refs() > 0,
                "parent node must contain ref to its child node"
            );

            if let ValueType::SpecifiedLocal { .. } = self.inner {
                parent.spec_refs = parent.spec_refs.saturating_sub(1);
            } else {
                parent.unspec_refs = parent.unspec_refs.saturating_sub(1);
            }

            // Update parent node
            GasTree::<T>::mutate(id, |value| {
                *value = Some(parent);
            });
        }

        Ok(())
    }

    fn move_value_upstream<T: Config>(&mut self) -> DispatchResult {
        if let ValueType::SpecifiedLocal {
            value: self_value,
            parent,
        } = self.inner
        {
            if self.unspec_refs == 0 {
                // This is specified, so it needs to get to the first specified parent also
                // going up until external or specified parent is found

                // `parent` key is known to exist, hence there must be it's ancestor with value.
                // If there isn't, the gas tree is considered corrupted (invalidated).
                let mut parents_ancestor = <Pallet<T>>::get_node(parent)
                    .ok_or(Error::<T>::GasTreeInvalidated)?
                    .node_with_value::<T>()?;

                // NOTE: intentional expect. A parents_ancestor is guaranteed to have inner_value
                let val = parents_ancestor
                    .inner_value_mut()
                    .expect("Querying parent with value");
                *val = val.saturating_add(self_value);
                *self
                    .inner_value_mut()
                    .expect("self is a type with a specified value") = 0;

                GasTree::<T>::mutate(parents_ancestor.id, |value| {
                    *value = Some(parents_ancestor);
                });
            }
        }
        Ok(())
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
        NodeAlreadyExists,

        /// Account doesn't have enough funds to complete operation.
        InsufficientBalance,

        /// Value node doesn't exist for a key
        NodeNotFound,

        /// Gas tree has been invalidated
        GasTreeInvalidated,

        /// Creating node with existing id
        KeyAlreadyExists,

        /// Procedure can't be called on consumed node
        NodeWasConsumed,
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

        /// Performs, if possible, cascade deletion of multiple nodes on the same path from the node with `key` id
        /// to the tree's root.
        ///
        /// There are two requirements for the node to be deleted:
        /// 1. Marked as consumed.
        /// 2. Has no child refs.
        ///
        /// If the node's type is `ValueType::External`, the locked value is released to the owner.
        /// Otherwise pre-delete ops are executed, then the node is deleted and after that the same procedure
        /// is repeated on the node's parent until it's marked consumed and has no child refs.
        ///
        /// Pre-delete ops are:
        /// 1. Parents refs decrease.
        /// 2. Value movement to the first parent, which can hold specified value.
        ///
        /// The latter op is required, because node can be marked as consumed, but still has non-zero inner value.
        /// That is the case, when node was splitted without gas and then consumed. So when node's gas-less child
        /// is consumed the [`Self::check_consumed`] function is called on the consumed parent with non-zero value.
        pub(super) fn check_consumed(key: H256) -> Result<ConsumeOutput<T>, DispatchError> {
            let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;
            while node.consumed && node.refs() == 0 {
                node.decrease_parents_ref::<T>()?;
                node.move_value_upstream::<T>()?;
                GasTree::<T>::remove(node.id);

                match node.inner {
                    ValueType::External { id, value } => {
                        return Ok(Some((NegativeImbalance::new(value), id)))
                    }
                    ValueType::SpecifiedLocal { parent, .. }
                    | ValueType::UnspecifiedLocal { parent } => {
                        node = Self::get_node(parent).ok_or(Error::<T>::GasTreeInvalidated)?;
                    }
                }
            }

            Ok(None)
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
            Error::<T>::NodeAlreadyExists
        );

        let node = ValueNode::new(origin, key, amount);

        // Save value node to storage
        GasTree::<T>::insert(key, node);

        Ok(PositiveImbalance::new(amount))
    }

    fn get_origin(key: H256) -> Result<Option<H256>, DispatchError> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            if let ValueNode {
                inner: ValueType::External { id, .. },
                ..
            } = node.root::<T>()?
            {
                Some(id)
            } else {
                unreachable!("Guaranteed by ValueNode::root method");
            }
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_origin_key(key: H256) -> Result<Option<H256>, DispatchError> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            node.root::<T>().map(|n| Some(n.id))?
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

    /// Marks a node with `key` as consumed and tries to delete it
    /// and all the nodes on the path from it to the root.
    ///
    /// Deletion of a node happens only if:
    /// 1. `Self::consume` was called on the node
    /// 2. The node has no children, i.e. spec/unspec refs.
    /// So if it's impossible to delete a node, then it's impossible to delete its parent in the current call.
    /// Also if it's possible to delete a node, then it doesn't necessarily mean that its parent will be deleted.
    /// An example here could be the case, when during async execution original message went to wait list, so wasn't consumed
    /// but the one generated during the execution of the original message went to message queue and was successfully executed.
    ///
    /// If a node under the `key` is of a `ValueType::SpecifiedLocal` and it has no unspec refs,
    /// then it moves the value up to the first ancestor, that can hold the value. If node has
    /// unspec ref, it means the unspec children rely on the gas held by the node, therefore value
    /// isn't moved up in this case.
    fn consume(key: H256) -> Result<ConsumeOutput<T>, DispatchError> {
        let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        ensure!(!node.consumed, Error::<T>::NodeWasConsumed);

        node.consumed = true;
        node.move_value_upstream::<T>()?;

        let outcome = if node.refs() == 0 {
            node.decrease_parents_ref::<T>()?;
            GasTree::<T>::remove(key);
            match node.inner {
                ValueType::UnspecifiedLocal { parent }
                | ValueType::SpecifiedLocal { parent, .. } => Self::check_consumed(parent)?,
                ValueType::External { id, value } => Some((NegativeImbalance::new(value), id)),
            }
        } else {
            // Save current node
            GasTree::<T>::mutate(key, |value| {
                *value = Some(node);
            });
            None
        };

        Ok(outcome)
    }

    /// Spends `amount` of gas from the ancestor of node with `key` id.
    ///
    /// Calling the function is possible even if an ancestor is consumed.
    ///
    /// ### Note:
    /// Node is considered as an ancestor of itself.
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

        // Save node that delivers limit
        GasTree::<T>::mutate(node.id, |value| {
            *value = Some(node);
        });

        Ok(NegativeImbalance::new(amount))
    }

    fn split(key: H256, new_node_key: H256) -> DispatchResult {
        let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        ensure!(!node.consumed, Error::<T>::NodeWasConsumed);
        // This also checks if key == new_node_key
        ensure!(
            !GasTree::<T>::contains_key(new_node_key),
            Error::<T>::NodeAlreadyExists
        );

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
        let mut parent = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        ensure!(!parent.consumed, Error::<T>::NodeWasConsumed);
        // This also checks if key == new_node_key
        ensure!(
            !GasTree::<T>::contains_key(new_node_key),
            Error::<T>::NodeAlreadyExists
        );

        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let mut ancestor_with_value = parent.node_with_value::<T>()?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        ensure!(
            ancestor_with_value
                .inner_value()
                .expect("Querying node with value")
                >= amount,
            Error::<T>::InsufficientBalance
        );

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

        parent.spec_refs = parent.spec_refs.saturating_add(1);
        if parent.id == ancestor_with_value.id {
            *parent.inner_value_mut().expect("Querying node with value") -= amount;
            GasTree::<T>::mutate(key, |value| {
                *value = Some(parent);
            });
        } else {
            // Update current node
            GasTree::<T>::mutate(key, |value| {
                *value = Some(parent);
            });
            *ancestor_with_value
                .inner_value_mut()
                .expect("Querying node with value") -= amount;
            GasTree::<T>::mutate(ancestor_with_value.id, |value| {
                *value = Some(ancestor_with_value);
            });
        }

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
