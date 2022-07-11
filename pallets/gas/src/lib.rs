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

// todo [sab] refactoring ideas
// 1. Change value node types (no need for spec refs for unspecified nodes)

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

#[cfg(test)]
mod property_tests;

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub enum ValueType {
    External { id: H256, value: u64 },
    SpecifiedLocal { parent: H256, value: u64 },
    UnspecifiedLocal { parent: H256 },
}

impl ValueType {
    fn is_external(&self) -> bool {
        matches!(self, ValueType::External { .. })
    }

    fn is_specified_local(&self) -> bool {
        matches!(self, ValueType::SpecifiedLocal { .. })
    }

    fn is_unspecified_local(&self) -> bool {
        matches!(self, ValueType::UnspecifiedLocal { .. })
    }
}

impl Default for ValueType {
    fn default() -> Self {
        ValueType::External {
            id: H256::default(),
            value: 0,
        }
    }
}

// todo [sab] UnspecifiedLocal don't need refs
// todo [sab] Check if ok to remove check key not consumed during split/split_with_value
// todo [sab] test splits with zero amount
// todo [sab] explore and extensively test cases when imbalances are used externally
// todo [sab] remove explicit errors if necessary (and invariant checks, or move invariant checks to separate checker fns)
// todo [sab] refactoring for consume/check_consumed idea - separate to 2 fns attemt to catch value and an attempt to remove node
#[derive(Clone, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct ValueNode {
    pub spec_refs: u32,
    pub unspec_refs: u32,
    pub inner: ValueType,
    pub consumed: bool,
}

impl ValueNode {
    pub fn new(origin: H256, value: u64) -> Self {
        Self {
            inner: ValueType::External { id: origin, value },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        }
    }

    // todo [sab] change when added new nodes
    /// Returns whether the node is patron or not
    ///
    /// The flag signals whether the node isn't available for the gas to be spent from it. These are nodes that:
    /// 1. Have unspec refs (regardless of being consumed).
    /// 2. Are not consumed.
    ///
    /// Patron nodes are those on which other nodes of the tree rely (including the self node).
    pub fn is_patron(&self) -> bool {
        !self.consumed || self.unspec_refs != 0
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

    /// Returns the first parent, that is able to hold a concrete value, but doesn't
    /// necessarily have a non-zero value, along with it's id
    ///
    /// Node itself is considered as a self-parent too. The gas tree holds invariant, that
    /// all the nodes with unspecified value always have a parent with a specified value.
    ///
    /// The id of the returned node is of `Option` type. If it's `None`, it means, that
    /// the ancestor and `self` are the same.
    pub fn node_with_value<T: Config>(self) -> Result<(ValueNode, Option<H256>), DispatchError> {
        let mut ret_node = self;
        let mut ret_id = None;
        if let ValueType::UnspecifiedLocal { parent } = ret_node.inner {
            ret_id = Some(parent);
            ret_node = <Pallet<T>>::get_node(parent).ok_or(Error::<T>::ParentIsLost)?;
        }

        Ok((ret_node, ret_id))
    }

    /// Returns id and data for root node (as [`ValueNode`]) of the value tree, which contains `self` node.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// As in [`ValueNode::node_with_value`], root's id is of `Option` type. It is equal to `None` in case
    /// `self` is a root node.
    pub fn root<T: Config>(self) -> Result<(Self, Option<H256>), DispatchError> {
        let mut ret_id = None;
        let mut ret_node = self;
        while let Some(parent) = ret_node.parent() {
            ret_id = Some(parent);
            ret_node = <Pallet<T>>::get_node(parent).ok_or(Error::<T>::ParentIsLost)?;
        }
        Ok((ret_node, ret_id))
    }

    fn decrease_parents_ref<T: Config>(&self) -> DispatchResult {
        if let Some(id) = self.parent() {
            let mut parent = <Pallet<T>>::get_node(id).ok_or(Error::<T>::ParentIsLost)?;
            ensure!(parent.refs() > 0, Error::<T>::ParentHasNoChildren,);

            match self.inner {
                ValueType::SpecifiedLocal { .. } => {
                    parent.spec_refs = parent.spec_refs.saturating_sub(1)
                }
                ValueType::UnspecifiedLocal { .. } => {
                    parent.unspec_refs = parent.unspec_refs.saturating_sub(1)
                }
                ValueType::External { .. } => {
                    unreachable!("node is guaranteed to have a parent, so can't be an external one")
                }
            }

            // Update parent node
            GasTree::<T>::insert(id, parent);
        }

        Ok(())
    }

    /// Tries to __"catch"__ the value inside the node if possible.
    ///
    /// If the node is a patron or of unspecified type, value is blocked, i.e.
    /// can't be removed or impossible to hold value to be removed.
    ///
    /// If the node is not a patron, but it has an ancestor patron, value is moved
    /// to it. So the patron's balance is increased (mutated). Otherwise the value
    /// is caught and removed from the tree. In both cases the `self` node's balance
    /// is zeroed.
    ///
    /// # Note
    /// Method doesn't mutate `self` in the storage, but only changes it's balance in memory.
    fn catch_value<T: Config>(&mut self) -> Result<CatchValueOutput, DispatchError> {
        if self.is_patron() {
            return Ok(CatchValueOutput::Blocked);
        }

        // todo [sab] check could be redundant after Tiany's PR
        if !self.inner.is_unspecified_local() {
            if let Some((mut patron, patron_id)) = self.find_ancestor_patron::<T>()? {
                let self_value = self
                    .inner_value_mut()
                    .expect("is not unspecified, so has value; qed");
                if *self_value == 0 {
                    // Early return to prevent redundant storage look-ups
                    return Ok(CatchValueOutput::Missed);
                }
                let patron_value = patron
                    .inner_value_mut()
                    .expect("Querying patron with value");
                *patron_value = patron_value.saturating_add(*self_value);
                *self_value = 0;
                GasTree::<T>::insert(patron_id, patron);

                Ok(CatchValueOutput::Missed)
            } else {
                let self_value = self
                    .inner_value_mut()
                    .expect("is not unspecified, so has value; qed");
                let value_copy = *self_value;
                *self_value = 0;

                Ok(CatchValueOutput::Caught(value_copy))
            }
        } else {
            Ok(CatchValueOutput::Blocked)
        }
    }

    /// Looks for `self` node's patron ancestor.
    ///
    /// A patron node is the node, on which some other nodes in the tree rely. More precisely,
    /// unspecified local nodes rely on nodes with value, so these specified nodes (`ValueType::External`, `ValueType::SpecifiedLocal`)
    /// are patron ones. The other criteria for a node to be marked as the patron one is not
    /// being consumed - value of such nodes mustn't be moved, because node itself rely on it.
    fn find_ancestor_patron<T: Config>(&self) -> Result<Option<(Self, H256)>, DispatchError> {
        match self.inner {
            ValueType::External { .. } => Ok(None),
            ValueType::SpecifiedLocal { parent, .. } => {
                let mut ret_id = parent;
                let mut ret_node = <Pallet<T>>::get_node(parent).ok_or(Error::<T>::ParentIsLost)?;
                while !ret_node.is_patron() {
                    match ret_node.inner {
                        ValueType::External { .. } => return Ok(None),
                        ValueType::SpecifiedLocal { parent, .. } => {
                            ret_id = parent;
                            ret_node =
                                <Pallet<T>>::get_node(parent).ok_or(Error::<T>::ParentIsLost)?;
                        }
                        _ => return Err(Error::<T>::UnexpectedNodeType.into()),
                    }
                }
                Ok(Some((ret_node, ret_id)))
            }
            // Although unspecified local type has a patron parent, it's considered
            // an error to call the method from that type of gas node.
            _ => Err(Error::<T>::UnexpectedNodeType.into()),
        }
    }
}

/// Output of `ValueNode::catch_value` call.
#[derive(Debug, Clone, Copy)]
enum CatchValueOutput {
    /// Catching value is impossible, therefore blocked.
    Blocked,
    /// Value was not caught, because was moved to the patron node.
    ///
    /// For more info about patron nodes see `ValueNode::find_ancestor_patron`
    Missed,
    /// Value was caught and will be removed from the node
    Caught(u64),
}

impl CatchValueOutput {
    fn into_consume_output<T: Config>(self, origin: H256) -> ConsumeOutput<T> {
        match self {
            CatchValueOutput::Caught(value) => Some((NegativeImbalance::new(value), origin)),
            _ => None,
        }
    }

    fn is_blocked(&self) -> bool {
        matches!(self, CatchValueOutput::Blocked)
    }

    fn is_caught(&self) -> bool {
        matches!(self, CatchValueOutput::Caught(_))
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

        UnexpectedConsumeOutput,

        UnexpectedNodeType,

        NodeIsNotPatron,

        ValueIsNotCaught,

        ValueIsBlocked,

        ValueIsNotBlocked,
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

        /// Tries to remove consumed nodes on the same path from the `key` node to the
        /// root (including it). While trying to remove nodes, also catches value stored
        /// in them is performed.
        ///
        /// Value catch is performed for all the non-patron nodes on the path from `key` to root,
        /// until some patron node is reached. By the invariant, catching can't be blocked,
        /// because the node is not a patron.
        ///
        /// For node removal there are 2 main requirements:
        /// 1. it's not a patron node
        /// 2. it doesn't have any children nodes.
        ///
        /// Although the value in nodes is moved or returned to the origin, calling `ValueNode::catch_value`
        /// in this procedure can still result in catching non-zero value. That's possible for example, when
        /// Gas-ful parent is consumed and has a gas-less child. When gas-less child is consumed in `ValueTree::consume`
        /// call, the gas-ful parent's value is caught in this function.
        ///
        /// # Invariants
        /// Internal invariant of the procedure:
        /// 1. If `catch_value` call ended up with `CatchValueOutput::Missed` in `consume`, all the calls of catch_value on ancestor nodes will be `CatchValueOutput::Missed` as well.
        /// 2. Also in that case cascade ancestors consumption will last until either the patron node or the first ancestor with specified child is found.
        /// 3. If `catch_value` call ended up with `CatchValueOutput::Caught(x)` in `consume`, all the calls of `catch_value` on ancestor nodes will be `CatchValueOutput::Caught(0)`.
        // todo [sab] check with proptest guarantees that ret_value !=0 (or == 0)
        pub(super) fn try_remove_consumed_ancestors(
            key: H256,
        ) -> Result<ConsumeOutput<T>, DispatchError> {
            let mut node_id = key;
            let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;
            let mut consume_output = None;
            let origin = Self::get_origin(key)?.expect("node with `key` the gas tree's part");

            while !node.is_patron() {
                let catch_output = node.catch_value::<T>()?;
                // The node is not a patron and can't be of unspecified type.
                ensure!(!catch_output.is_blocked(), Error::<T>::ValueIsBlocked,);

                // todo [sab] пойми, что ничего не упустил (вэлью не потерял) тут на тестах
                consume_output =
                    consume_output.or_else(|| catch_output.into_consume_output(origin));

                if node.spec_refs == 0 {
                    node.decrease_parents_ref::<T>()?;
                    GasTree::<T>::remove(node_id);

                    match node.inner {
                        ValueType::External { .. } => {
                            ensure!(catch_output.is_caught(), Error::<T>::ValueIsNotCaught,);
                            return Ok(consume_output);
                        }
                        ValueType::SpecifiedLocal { parent, .. } => {
                            node_id = parent;
                            node = Self::get_node(parent).ok_or(Error::<T>::ParentIsLost)?;
                        }
                        _ => return Err(Error::<T>::UnexpectedNodeType.into()),
                    }
                } else {
                    GasTree::<T>::insert(node_id, node);
                    return Ok(consume_output);
                }
            }

            Ok(consume_output)
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

        let node = ValueNode::new(origin, amount);

        // Save value node to storage
        GasTree::<T>::insert(key, node);

        Ok(PositiveImbalance::new(amount))
    }

    fn get_origin(key: H256) -> Result<Option<H256>, DispatchError> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            let (root, _) = node.root::<T>()?;
            if let ValueNode {
                inner: ValueType::External { id, .. },
                ..
            } = root
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
            node.root::<T>().map(|(_, id)| Some(id.unwrap_or(key)))?
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_limit(key: H256) -> Result<Option<u64>, DispatchError> {
        if let Some(node) = Self::get_node(key) {
            Ok({
                let (node_with_value, _) = node.node_with_value::<T>()?;
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

    /// Marks a node with `key` as consumed, tries to return it's value and
    /// delete it. The function performs same procedure with all the nodes on
    /// the path from it to the root, if possible.
    ///
    /// When consuming the node, it's value is mutated by calling `catch_value`, which
    /// tries to either return or move value upstream if possible. For more info, read
    /// the `catch_value` function's documentation.
    ///
    /// Deletion of a node happens only if:
    /// 1. `Self::consume` was called on the node
    /// 2. The node has no children, i.e. spec/unspec refs.
    /// So if it's impossible to delete a node, then it's impossible to delete its parent in the current call.
    /// Also if it's possible to delete a node, then it doesn't necessarily mean that its parent will be deleted.
    /// An example here could be the case, when during async execution original message went to wait list, so wasn't consumed
    /// but the one generated during the execution of the original message went to message queue and was successfully executed.
    fn consume(key: H256) -> Result<ConsumeOutput<T>, DispatchError> {
        let mut node = Self::get_node(key).ok_or(Error::<T>::NodeNotFound)?;

        ensure!(!node.consumed, Error::<T>::NodeWasConsumed,);

        node.consumed = true;
        let catch_output = node.catch_value::<T>()?;
        let origin = Self::get_origin(key)?.expect("existing node always has the origin");

        Ok(if node.refs() == 0 {
            node.decrease_parents_ref::<T>()?;
            GasTree::<T>::remove(key);

            match node.inner {
                ValueType::External { .. } => {
                    ensure!(catch_output.is_caught(), Error::<T>::ValueIsNotCaught);
                    catch_output.into_consume_output(origin)
                }
                ValueType::UnspecifiedLocal { parent } => {
                    ensure!(catch_output.is_blocked(), Error::<T>::ValueIsNotBlocked,);
                    Self::try_remove_consumed_ancestors(parent)?
                }
                ValueType::SpecifiedLocal { parent, .. } => {
                    ensure!(!catch_output.is_blocked(), Error::<T>::ValueIsBlocked,);
                    let consume_output = catch_output.into_consume_output(origin);
                    let consume_ancestors_output = Self::try_remove_consumed_ancestors(parent)?;
                    match (&consume_output, consume_ancestors_output) {
                        // value can't be caught in both procedures
                        (Some(_), Some((neg_imb, _))) if neg_imb == NegativeImbalance::zero() => {
                            consume_output
                        }
                        (None, None) => consume_output,
                        _ => return Err(Error::<T>::UnexpectedConsumeOutput.into()),
                    }
                }
            }
        } else {
            ensure!(
                node.inner.is_external() || node.inner.is_specified_local(),
                Error::<T>::UnexpectedNodeType
            );

            GasTree::<T>::insert(key, node);
            catch_output.into_consume_output(origin)
        })
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
        let (mut node, node_id) = Self::get_node(key)
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
        GasTree::<T>::insert(node_id.unwrap_or(key), node);

        Ok(NegativeImbalance::new(amount))
    }

    fn split(key: H256, new_node_key: H256) -> DispatchResult {
        let (mut node, node_id) = Self::get_node(key)
            .ok_or(Error::<T>::NodeNotFound)?
            .node_with_value::<T>()?;
        let node_id = node_id.unwrap_or(key);

        // This also checks if key == new_node_key
        ensure!(
            !GasTree::<T>::contains_key(new_node_key),
            Error::<T>::NodeAlreadyExists
        );

        node.unspec_refs = node.unspec_refs.saturating_add(1);

        let new_node = ValueNode {
            inner: ValueType::UnspecifiedLocal { parent: node_id },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        GasTree::<T>::insert(new_node_key, new_node);
        // Update current node
        GasTree::<T>::insert(node_id, node);

        Ok(())
    }

    fn split_with_value(key: H256, new_node_key: H256, amount: u64) -> DispatchResult {
        let (mut node, node_id) = Self::get_node(key)
            .ok_or(Error::<T>::NodeNotFound)?
            .node_with_value::<T>()?;
        let node_id = node_id.unwrap_or(key);

        // This also checks if key == new_node_key
        ensure!(
            !GasTree::<T>::contains_key(new_node_key),
            Error::<T>::NodeAlreadyExists
        );

        // NOTE: intentional expect. A `node` is guaranteed to have inner_value
        ensure!(
            node.inner_value().expect("Querying node with value") >= amount,
            Error::<T>::InsufficientBalance
        );

        let new_node = ValueNode {
            inner: ValueType::SpecifiedLocal {
                value: amount,
                parent: node_id,
            },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };
        // Save new node
        GasTree::<T>::insert(new_node_key, new_node);

        node.spec_refs = node.spec_refs.saturating_add(1);
        *node.inner_value_mut().expect("Querying node with value") -= amount;
        GasTree::<T>::insert(node_id, node);

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
