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

    /// The first upstream node (self included), that is able to hold a concrete value, but doesn't
    /// necessarily have a non-zero value.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// Returns tuple of two values, where:
    /// - first value is an ancestor, which has a specified gas amount
    /// - second value is the id of the ancestor.
    /// The latter value is of `Option` type. If it's `None`, it means, that the ancestor and `self`
    /// are the same.
    pub(super) fn node_with_value(
        node: StorageMap::Value,
    ) -> Result<(StorageMap::Value, Option<MapKey>), Error> {
        let mut ret_node = node;
        let mut ret_id = None;
        while let GasNodeType::UnspecifiedLocal { parent } = ret_node.inner {
            ret_id = Some(parent);
            ret_node = Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?;
        }

        Ok((ret_node, ret_id))
    }

    /// Returns id and data for root node (as [`ValueNode`]) of the value tree, which contains `self` node.
    /// If some node along the upstream path is missing, returns an error (tree is invalidated).
    ///
    /// As in [`ValueNode::node_with_value`], root's id is of `Option` type. It is equal to `None` in case
    /// `self` is a root node.
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
            GasNodeType::ReservedLocal { .. } => {
                unreachable!("node is guaranteed to have a parent, so can't be an reserved one")
            }
            GasNodeType::External { .. } => {
                unreachable!("node is guaranteed to have a parent, so can't be an external one")
            }
        }

        // Update parent node
        StorageMap::insert(id, parent);

        Ok(())
    }

    /// If `self` is of `ValueType::SpecifiedLocal` type, moves value upstream
    /// to the first ancestor, that can hold the value, in case `self` has not
    /// unspec children refs.
    ///
    /// This method is actually one of pre-delete procedures called when node is consumed.
    ///
    /// # Note
    /// Method doesn't mutate `self` in the storage, but only changes it's balance in memory.
    pub(super) fn move_value_upstream(node: &mut StorageMap::Value) -> Result<(), Error> {
        if node.unspec_refs != 0 {
            return Ok(());
        }

        if let GasNodeType::SpecifiedLocal {
            value: self_value,
            parent,
        } = node.inner
        {
            // This is specified, so it needs to get to the first specified parent also
            // going up until external or specified parent is found

            // `parent` key is known to exist, hence there must be it's ancestor with value.
            // If there isn't, the gas tree is considered corrupted (invalidated).
            let (mut parents_ancestor, ancestor_id) = Self::node_with_value(
                Self::get_node(parent).ok_or_else(InternalError::parent_is_lost)?,
            )?;

            // NOTE: intentional expect. A parents_ancestor is guaranteed to have inner_value
            let val = parents_ancestor
                .inner_value_mut()
                .expect("Querying parent with value");
            *val = val.saturating_add(self_value);
            *node
                .inner_value_mut()
                .expect("self is a type with a specified value") = Zero::zero();

            StorageMap::insert(ancestor_id.unwrap_or(parent), parents_ancestor);
        }
        Ok(())
    }

    pub(super) fn check_consumed(
        key: MapKey,
    ) -> Result<ConsumeOutput<NegativeImbalance<Balance, TotalValue>, ExternalId>, Error> {
        let mut node_id = key;
        let mut node = Self::get_node(node_id).ok_or_else(InternalError::node_not_found)?;
        while node.consumed && node.refs() == 0 {
            Self::decrease_parents_ref(&node)?;
            Self::move_value_upstream(&mut node)?;
            StorageMap::remove(node_id);

            match node.inner {
                GasNodeType::External { id, value } => {
                    return Ok(Some((NegativeImbalance::new(value), id)))
                }
                GasNodeType::SpecifiedLocal { parent, .. }
                | GasNodeType::UnspecifiedLocal { parent } => {
                    node_id = parent;
                    node = Self::get_node(node_id).ok_or_else(InternalError::parent_is_lost)?;
                }
                GasNodeType::ReservedLocal { .. } => {
                    unreachable!(
                        "node is guaranteed to be a parent, but reserved nodes have no children"
                    )
                }
            }
        }

        Ok(None)
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
        let mut parent = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;

        // Check if the parent node is reserved
        if let GasNodeType::ReservedLocal { .. } = parent.inner {
            return Err(InternalError::forbidden().into());
        }

        if parent.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_node_key) {
            return Err(InternalError::node_already_exists().into());
        }

        // Detect inner from `reserve`.
        let inner = if reserve {
            let id = Self::get_external(key)?.ok_or_else(InternalError::parent_is_lost)?;
            GasNodeType::ReservedLocal { id, value: amount }
        } else {
            parent.spec_refs = parent.spec_refs.saturating_add(1);

            GasNodeType::SpecifiedLocal {
                value: amount,
                parent: key,
            }
        };

        // Upstream node with a concrete value exist for any node.
        // If it doesn't, the tree is considered invalidated.
        let (mut ancestor_with_value, ancestor_id) = Self::node_with_value(parent.clone())?;

        // NOTE: intentional expect. A node_with_value is guaranteed to have inner_value
        if ancestor_with_value
            .inner_value()
            .expect("Querying node with value")
            < amount
        {
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

        if let Some(ancestor_id) = ancestor_id {
            // Update current node
            StorageMap::insert(key, parent);
            *ancestor_with_value
                .inner_value_mut()
                .expect("Querying node with value") -= amount;
            StorageMap::insert(ancestor_id, ancestor_with_value);
        } else {
            // parent and ancestor nodes are the same
            *parent.inner_value_mut().expect("Querying node with value") -= amount;
            StorageMap::insert(key, parent);
        }

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

    fn consume(
        key: Self::Key,
    ) -> Result<ConsumeOutput<Self::NegativeImbalance, Self::ExternalOrigin>, Self::Error> {
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        if node.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        node.consumed = true;
        Self::move_value_upstream(&mut node)?;

        Ok(if node.refs() == 0 {
            Self::decrease_parents_ref(&node)?;
            StorageMap::remove(key);
            match node.inner {
                GasNodeType::UnspecifiedLocal { parent }
                | GasNodeType::SpecifiedLocal { parent, .. } => Self::check_consumed(parent)?,
                GasNodeType::ReservedLocal { id, value } | GasNodeType::External { id, value } => {
                    Some((NegativeImbalance::new(value), id))
                }
            }
        } else {
            // Save current node
            StorageMap::insert(key, node);
            None
        })
    }

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
        let mut node = Self::get_node(key).ok_or_else(InternalError::node_not_found)?;
        // Check if the value node is reserved
        if let GasNodeType::ReservedLocal { .. } = node.inner {
            return Err(InternalError::forbidden().into());
        }

        if node.consumed {
            return Err(InternalError::node_was_consumed().into());
        }

        // This also checks if key == new_node_key
        if StorageMap::contains_key(&new_key) {
            return Err(InternalError::node_already_exists().into());
        }

        node.unspec_refs = node.unspec_refs.saturating_add(1);

        let new_node = GasNode {
            inner: GasNodeType::UnspecifiedLocal { parent: key },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        };

        // Save new node
        StorageMap::insert(new_key, new_node);
        // Update current node
        StorageMap::insert(key, node);

        Ok(())
    }

    fn cut(key: Self::Key, new_key: Self::Key, amount: Self::Balance) -> Result<(), Self::Error> {
        Self::create_from_with_value(key, new_key, amount, true)
    }
}
