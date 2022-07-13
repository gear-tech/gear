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

/// Output of `ValueNode::catch_value` call.
#[derive(Debug, Clone, Copy)]
enum CatchValueOutput<Balance> {
    /// Catching value is impossible, therefore blocked.
    Blocked,
    /// Value was not caught, because was moved to the patron node.
    ///
    /// For more info about patron nodes see `ValueNode::find_ancestor_patron`
    Missed,
    /// Value was caught and will be removed from the node
    Caught(Balance),
}

impl<Balance: BalanceTrait> CatchValueOutput<Balance> {
    fn into_consume_output<TotalValue, ExternalId>(
        self,
        origin: ExternalId,
    ) -> ConsumeOutput<NegativeImbalance<Balance, TotalValue>, ExternalId>
    where
        TotalValue: ValueStorage<Value = Balance>,
        ExternalId: Clone,
    {
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

pub struct TreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>(
    PhantomData<(TotalValue, InternalError, Error, ExternalId, StorageMap)>,
);

impl<TotalValue, Balance, InternalError, Error, MapKey, ExternalId, StorageMap>
    TreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>
where
    Balance: BalanceTrait,
    TotalValue: ValueStorage<Value = Balance>,
    InternalError: super::Error,
    Error: From<InternalError>,
    ExternalId: Clone,
    MapKey: Copy,
    StorageMap:
        super::storage::MapStorage<Key = MapKey, Value = GasNode<ExternalId, MapKey, Balance>>,
{
    pub(super) fn get_node(key: MapKey) -> Option<StorageMap::Value> {
        StorageMap::get(&key)
    }

    /// Returns the first parent, that is able to hold a concrete value, but doesn't
    /// necessarily have a non-zero value, along with it's id
    ///
    /// Node itself is considered as a self-parent too. The gas tree holds invariant, that
    /// all the nodes with unspecified value always have a parent with a specified value.
    ///
    /// The id of the returned node is of `Option` type. If it's `None`, it means, that
    /// the ancestor and `self` are the same.
    pub(super) fn node_with_value(
        node: StorageMap::Value,
    ) -> Result<(StorageMap::Value, Option<MapKey>), Error> {
        let mut ret_node = node;
        let mut ret_id = None;
        if let GasNodeType::UnspecifiedLocal { parent } = ret_node.inner {
            ret_id = Some(parent);
            ret_node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
        }

        Ok((ret_node, ret_id))
    }

    /// Returns id and data for root node (as [`GasNode`]) of the value tree, which contains the `node`.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// As in [`TreeImpl::node_with_value`], root's id is of `Option` type. It is equal to `None` in case
    /// `node` is a root node.
    pub(super) fn root(
        node: StorageMap::Value,
    ) -> Result<(StorageMap::Value, Option<MapKey>), Error> {
        let mut ret_id = None;
        let mut ret_node = node;
        while let Some(parent) = ret_node.parent() {
            ret_id = Some(parent);
            ret_node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
        }

        Ok((ret_node, ret_id))
    }

    pub(super) fn decrease_parents_ref(node: &StorageMap::Value) -> Result<(), Error> {
        let id = match node.parent() {
            Some(id) => id,
            None => return Ok(()),
        };

        let mut parent = Self::get_node(id).ok_or_else(InternalError::parent_is_lost)?;
        if parent.refs() == 0 {
            return Err(InternalError::parent_has_no_children().into());
        }

        match node.inner {
            GasNodeType::SpecifiedLocal { .. } => {
                parent.spec_refs = parent.spec_refs.saturating_sub(1)
            }
            GasNodeType::UnspecifiedLocal { .. } => {
                parent.unspec_refs = parent.unspec_refs.saturating_sub(1)
            }
            _ => return Err(InternalError::unexpected_node_type().into()),
        }

        // Update parent node
        StorageMap::insert(id, parent);

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
    fn catch_value(node: &mut StorageMap::Value) -> Result<CatchValueOutput<Balance>, Error> {
        if node.is_patron() {
            return Ok(CatchValueOutput::Blocked);
        }

        if !node.inner.is_unspecified_local() {
            if let Some((mut patron, patron_id)) = Self::find_ancestor_patron(node)? {
                let self_value = node
                    .inner_value_mut()
                    .expect("is not unspecified, so has value; qed");
                if *self_value == Zero::zero() {
                    // Early return to prevent redundant storage look-ups
                    return Ok(CatchValueOutput::Missed);
                }
                let patron_value = patron
                    .inner_value_mut()
                    .expect("Querying patron with value");
                *patron_value = patron_value.saturating_add(*self_value);
                *self_value = Zero::zero();
                StorageMap::insert(patron_id, patron);

                Ok(CatchValueOutput::Missed)
            } else {
                let self_value = node
                    .inner_value_mut()
                    .expect("is not unspecified, so has value; qed");
                let value_copy = *self_value;
                *self_value = Zero::zero();

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
    fn find_ancestor_patron(
        node: &StorageMap::Value,
    ) -> Result<Option<(StorageMap::Value, MapKey)>, Error> {
        match node.inner {
            GasNodeType::External { .. } | GasNodeType::ReservedLocal { .. } => Ok(None),
            GasNodeType::SpecifiedLocal { parent, .. } => {
                let mut ret_id = parent;
                let mut ret_node =
                    Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
                while !ret_node.is_patron() {
                    match ret_node.inner {
                        GasNodeType::External { .. } => return Ok(None),
                        GasNodeType::SpecifiedLocal { parent, .. } => {
                            ret_id = parent;
                            ret_node =
                                Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
                        }
                        _ => return Err(InternalError::unexpected_node_type().into()),
                    }
                }
                Ok(Some((ret_node, ret_id)))
            }
            // Although unspecified local type has a patron parent, it's considered
            // an error to call the method from that type of gas node.
            GasNodeType::UnspecifiedLocal { .. } => Err(InternalError::forbidden().into()),
        }
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
    pub(super) fn try_remove_consumed_ancestors(
        key: MapKey,
    ) -> Result<ConsumeOutput<NegativeImbalance<Balance, TotalValue>, ExternalId>, Error> {
        let mut node_id = key;
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        let mut consume_output = None;
        let (_, origin) = Self::get_origin(key)?.expect("node with `key` the gas tree's part");

        while !node.is_patron() {
            let catch_output = Self::catch_value(&mut node)?;
            // The node is not a patron and can't be of unspecified type.
            if catch_output.is_blocked() {
                return Err(InternalError::value_is_blocked().into());
            }

            consume_output =
                consume_output.or_else(|| catch_output.into_consume_output(origin.clone()));

            if node.spec_refs == 0 {
                Self::decrease_parents_ref(&node)?;
                StorageMap::remove(node_id);

                match node.inner {
                    GasNodeType::External { .. } => {
                        if !catch_output.is_caught() {
                            return Err(InternalError::value_is_not_caught().into());
                        }
                        return Ok(consume_output);
                    }
                    GasNodeType::SpecifiedLocal { parent, .. } => {
                        node_id = parent;
                        node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
                    }
                    _ => return Err(InternalError::unexpected_node_type().into()),
                }
            } else {
                StorageMap::insert(node_id, node);
                return Ok(consume_output);
            }
        }

        Ok(consume_output)
    }

    /// Create ValueNode from node key with value
    ///
    /// if `reserve`, create ValueType::ReservedLocal
    /// else, create ValueType::SpecifiedLocal
    pub(super) fn create_from_with_value(
        key: MapKey,
        new_node_key: MapKey,
        amount: Balance,
        reserve: bool,
    ) -> Result<(), Error> {
        let (mut node, node_id) =
            Self::node_with_value(Self::get_node(key).ok_or_else(InternalError::node_not_found)?)?;
        let node_id = node_id.unwrap_or(key);

        // Check if the parent node is reserved
        if node.inner.is_reserved_local() {
            return Err(InternalError::forbidden().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_node_key) {
            return Err(InternalError::node_already_exists().into());
        }

        // Detect inner from `reserve`.
        let inner = if reserve {
            let id = Self::get_external(key)?.expect("existing node always have origin");
            GasNodeType::ReservedLocal { id, value: amount }
        } else {
            node.spec_refs = node.spec_refs.saturating_add(1);

            GasNodeType::SpecifiedLocal {
                value: amount,
                parent: node_id,
            }
        };

        // NOTE: intentional expect. A `node is guaranteed to have inner_value
        if node.inner_value().expect("Querying node with value") < amount {
            return Err(InternalError::insufficient_balance().into());
        }

        let new_node = GasNode {
            inner,
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        StorageMap::insert(new_node_key, new_node);

        *node.inner_value_mut().expect("Querying node with value") -= amount;
        StorageMap::insert(node_id, node);

        Ok(())
    }
}

impl<TotalValue, Balance, InternalError, Error, MapKey, ExternalId, StorageMap> Tree
    for TreeImpl<TotalValue, InternalError, Error, ExternalId, StorageMap>
where
    Balance: BalanceTrait,
    TotalValue: ValueStorage<Value = Balance>,
    InternalError: super::Error,
    Error: From<InternalError>,
    ExternalId: Clone,
    MapKey: Copy,
    StorageMap:
        super::storage::MapStorage<Key = MapKey, Value = GasNode<ExternalId, MapKey, Balance>>,
{
    type ExternalOrigin = ExternalId;
    type Key = MapKey;
    type Balance = Balance;

    type PositiveImbalance = PositiveImbalance<Balance, TotalValue>;
    type NegativeImbalance = NegativeImbalance<Balance, TotalValue>;

    type InternalError = InternalError;
    type Error = Error;

    fn total_supply() -> Self::Balance {
        TotalValue::get().unwrap_or_else(Zero::zero)
    }

    fn create(
        origin: Self::ExternalOrigin,
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, Self::Error> {
        if StorageMap::contains_key(&key) {
            return Err(InternalError::node_already_exists().into());
        }

        let node = GasNode::new(origin, amount);

        // Save value node to storage
        StorageMap::insert(key, node);

        Ok(PositiveImbalance::new(amount))
    }

    fn get_origin(
        key: Self::Key,
    ) -> Result<OriginResult<Self::Key, Self::ExternalOrigin>, Self::Error> {
        Ok(if let Some(node) = Self::get_node(key) {
            // key known, must return the origin, unless corrupted
            let (root, maybe_key) = Self::root(node)?;
            match root.inner {
                GasNodeType::External { id, .. } | GasNodeType::ReservedLocal { id, .. } => {
                    Some((maybe_key.unwrap_or(key), id))
                }
                _ => unreachable!("Guaranteed by ValueNode::root method"),
            }
        } else {
            // key unknown - legitimate result
            None
        })
    }

    fn get_limit(key: Self::Key) -> Result<Option<(Self::Balance, Self::Key)>, Self::Error> {
        if let Some(node) = Self::get_node(key) {
            Ok({
                let (node_with_value, maybe_key) = Self::node_with_value(node)?;
                // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
                let v = node_with_value
                    .inner_value()
                    .expect("The node here is either external or specified, hence the inner value");
                Some((v, maybe_key.unwrap_or(key)))
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
    fn consume(
        key: Self::Key,
    ) -> Result<ConsumeOutput<Self::NegativeImbalance, Self::ExternalOrigin>, Self::Error> {
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;

        if node.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        node.consumed = true;
        let catch_output = Self::catch_value(&mut node)?;
        let (_, origin) = Self::get_origin(key)?.expect("existing node always has the origin");

        Ok(if node.refs() == 0 {
            Self::decrease_parents_ref(&node)?;
            StorageMap::remove(key);

            match node.inner {
                GasNodeType::External { .. } | GasNodeType::ReservedLocal { .. } => {
                    if !catch_output.is_caught() {
                        return Err(InternalError::value_is_not_caught().into());
                    }
                    catch_output.into_consume_output(origin)
                }
                GasNodeType::UnspecifiedLocal { parent } => {
                    if !catch_output.is_blocked() {
                        return Err(InternalError::value_is_not_blocked().into());
                    }
                    Self::try_remove_consumed_ancestors(parent)?
                }
                GasNodeType::SpecifiedLocal { parent, .. } => {
                    if catch_output.is_blocked() {
                        return Err(InternalError::value_is_blocked().into());
                    }
                    let consume_output = catch_output.into_consume_output(origin);
                    let consume_ancestors_output = Self::try_remove_consumed_ancestors(parent)?;
                    match (&consume_output, consume_ancestors_output) {
                        // value can't be caught in both procedures
                        (Some(_), Some((neg_imb, _))) if neg_imb.peek() == Zero::zero() => {
                            consume_output
                        }
                        (None, None) => consume_output,
                        _ => return Err(InternalError::unexpected_consume_output().into()),
                    }
                }
            }
        } else {
            if node.inner.is_reserved_local() || node.inner.is_unspecified_local() {
                return Err(InternalError::unexpected_node_type().into());
            }

            StorageMap::insert(key, node);
            catch_output.into_consume_output(origin)
        })
    }

    /// Spends `amount` of gas from the ancestor of node with `key` id.
    ///
    /// Calling the function is possible even if an ancestor is consumed.
    ///
    /// ### Note:
    /// Node is considered as an ancestor of itself.
    fn spend(
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::NegativeImbalance, Self::Error> {
        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let (mut node, node_id) =
            Self::node_with_value(Self::get_node(key).ok_or_else(InternalError::node_not_found)?)?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        if node.inner_value().expect("Querying node with value") < amount {
            return Err(InternalError::insufficient_balance().into());
        }

        *node.inner_value_mut().expect("Querying node with value") -= amount;
        log::debug!("Spent {:?} of gas", amount);

        // Save node that delivers limit
        StorageMap::insert(node_id.unwrap_or(key), node);

        Ok(NegativeImbalance::new(amount))
    }

    fn split_with_value(
        key: Self::Key,
        new_key: Self::Key,
        amount: Self::Balance,
    ) -> Result<(), Self::Error> {
        Self::create_from_with_value(key, new_key, amount, false)
    }

    fn split(key: Self::Key, new_key: Self::Key) -> Result<(), Self::Error> {
        let (mut node, node_id) =
            Self::node_with_value(Self::get_node(key).ok_or_else(InternalError::node_not_found)?)?;
        let node_id = node_id.unwrap_or(key);

        // Check if the value node is reserved
        if node.inner.is_reserved_local() {
            return Err(InternalError::forbidden().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_key) {
            return Err(InternalError::node_already_exists().into());
        }

        node.unspec_refs = node.unspec_refs.saturating_add(1);

        let new_node = GasNode {
            inner: GasNodeType::UnspecifiedLocal { parent: node_id },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        StorageMap::insert(new_key, new_node);
        // Update current node
        StorageMap::insert(node_id, node);

        Ok(())
    }

    fn cut(key: Self::Key, new_key: Self::Key, amount: Self::Balance) -> Result<(), Self::Error> {
        Self::create_from_with_value(key, new_key, amount, true)
    }
}
