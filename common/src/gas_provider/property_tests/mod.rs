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

//! Properties and invariants that are checked:
//!
//! 1. Nodes can become consumed only after [`Tree::consume`] call.
//!
//! 2. Unspec refs counter for the current node is incremented only after
//! [`Tree::split`] which creates a node with [`GasNode::UnspecifiedLocal`]
//! type.
//!
//! 3. Spec refs counter for the current node is incremented only after
//! [`GasNode::split_with_value`], which creates a node with
//! [`GasNode::SpecifiedLocal`] type.
//!
//! 4. All nodes, except for [`GasNode::Cut`], [`GasNode::Reserve`] and
//! [`GasNode::External`] have a parent in GasTree storage.
//!
//! 5. All nodes with parent point to a parent with value. So if a `key` is an
//! id of [`GasNode::SpecifiedLocal`], [`GasNode::Reserved`] or [`GasNode::External`] node,
//! the node under this `key` will always be a parent of the newly generated node
//! after [`Tree::split`]/[`Tree::split_with_value`] call.
//! However, there is no such guarantee if key is an id of the
//! [`GasNode::UnspecifiedLocal`] nodes.
//!
//! 6. All non-external nodes have ancestor with value (for example,
//! [`TreeImpl::node_with_value`] procedure always return `Ok`),
//! however this value can be equal to 0.
//! This ancestor is either a parent or the node itself.
//!
//! 7. All nodes can't have consumed parent with zero refs (there can't be any
//! nodes like that in storage) between calls to [`Tree::consume`]. Therefore,
//! if node is deleted, it is consumed and has zero refs (and zero value).
//!
//! 8. [`GasNode::UnspecifiedLocal`] nodes are always leaves in the tree (they
//! have no children), so they are always deleted after consume call. The same
//! rule is for [`GasNode::Cut`] nodes. So there can't be any
//! [`GasNode::UnspecifiedLocal`] node in the tree with consumed field
//! set to true. So if there is an **existing consumed** node, then it
//! has non-zero refs counter and a value >= 0 (between calls to
//! [`Tree::consume`]).
//!
//! 9. In a tree a root with [`GasNode::External`] or [`GasNode::Reserved`]
//! type is always deleted last.
//!
//! 10. If node wasn't removed after `consume` it's [`GasNode::SpecifiedLocal`],
//! [`GasNode::Reserved`] or [`GasNode::External`] node. Similar to the previous invariant, but
//! focuses more on [`Tree::consume`] procedure, while the other focuses
//! on the all tree invariant. (checked in `consume` call assertions).
//!
//! 11. [`GasNode::UnspecifiedLocal`] and [`GasNode::Cut`] nodes can't
//! be removed, nor mutated during cascade removal. So after [`Tree::consume`]
//! call not more than one node is of [`GasNode::UnspecifiedLocal`] type.
//!
//! 12. Between calls to [`Tree::consume`] if node is consumed and has no unspec
//! refs, it's internal gas value is zero.
//!
//! 13. Between calls to [`Tree::consume`] if node has value, it's either not
//! consumed or it has unspecified children.
//!
//! 14. Value catch can be performed only on consumed nodes (not tested).

use super::*;
use crate::storage::MapStorage;
use core::{cell::RefCell, iter::FromIterator, ops::DerefMut};
use frame_support::{assert_err, assert_ok};
use gear_utils::{NonEmpty, RingGet};
use primitive_types::H256;
use proptest::prelude::*;
use std::collections::HashMap;
use strategies::GasTreeAction;

mod assertions;
mod strategies;
mod utils;

type Balance = u64;

std::thread_local! {
    static TOTAL_ISSUANCE: RefCell<Option<Balance>> = RefCell::new(None);
}

#[derive(Debug, PartialEq, Eq)]
struct TotalIssuanceWrap;

impl ValueStorage for TotalIssuanceWrap {
    type Value = Balance;

    fn exists() -> bool {
        TOTAL_ISSUANCE.with(|i| i.borrow().is_some())
    }

    fn get() -> Option<Self::Value> {
        TOTAL_ISSUANCE.with(|i| *i.borrow())
    }

    fn kill() {
        TOTAL_ISSUANCE.with(|i| {
            *i.borrow_mut() = None;
        })
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R {
        TOTAL_ISSUANCE.with(|i| f(i.borrow_mut().deref_mut()))
    }

    fn put(value: Self::Value) {
        TOTAL_ISSUANCE.with(|i| {
            i.replace(Some(value));
        })
    }

    fn set(value: Self::Value) -> Option<Self::Value> {
        Self::mutate(|opt| {
            let prev = opt.take();
            *opt = Some(value);
            prev
        })
    }

    fn take() -> Option<Self::Value> {
        TOTAL_ISSUANCE.with(|i| i.take())
    }
}

type Key = GasNodeId<MapKey, ReservationKey>;
type GasNode = super::GasNode<ExternalOrigin, Key, Balance>;

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExternalOrigin(MapKey);

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct MapKey(H256);

impl MapKey {
    fn random() -> Self {
        Self(H256::random())
    }
}

impl<U> From<MapKey> for GasNodeId<MapKey, U> {
    fn from(key: MapKey) -> Self {
        Self::Node(key)
    }
}

#[derive(Debug, Copy, Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ReservationKey(H256);

impl ReservationKey {
    fn random() -> Self {
        Self(H256::random())
    }
}

impl<T> From<ReservationKey> for GasNodeId<T, ReservationKey> {
    fn from(key: ReservationKey) -> Self {
        Self::Reservation(key)
    }
}

std::thread_local! {
    static GAS_TREE_NODES: RefCell<BTreeMap<Key, GasNode>> = RefCell::new(BTreeMap::new());
}

struct GasTreeNodesWrap;

impl storage::MapStorage for GasTreeNodesWrap {
    type Key = Key;
    type Value = GasNode;

    fn contains_key(key: &Self::Key) -> bool {
        GAS_TREE_NODES.with(|tree| tree.borrow().contains_key(key))
    }

    fn get(key: &Self::Key) -> Option<Self::Value> {
        GAS_TREE_NODES.with(|tree| tree.borrow().get(key).cloned())
    }

    fn insert(key: Self::Key, value: Self::Value) {
        GAS_TREE_NODES.with(|tree| tree.borrow_mut().insert(key, value));
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(_key: Self::Key, _f: F) -> R {
        unimplemented!()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut _f: F) {
        unimplemented!()
    }

    fn remove(key: Self::Key) {
        GAS_TREE_NODES.with(|tree| tree.borrow_mut().remove(&key));
    }

    fn clear() {
        GAS_TREE_NODES.with(|tree| tree.borrow_mut().clear());
    }

    fn take(key: Self::Key) -> Option<Self::Value> {
        GAS_TREE_NODES.with(|tree| tree.borrow_mut().remove(&key))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    NodeAlreadyExists,
    ParentIsLost,
    ParentHasNoChildren,
    NodeNotFound,
    NodeWasConsumed,
    InsufficientBalance,
    Forbidden,
    UnexpectedConsumeOutput,
    UnexpectedNodeType,
    ValueIsNotCaught,
    ValueIsBlocked,
    ValueIsNotBlocked,
    ConsumedWithLock,
    ConsumedWithSystemReservation,
    TotalValueIsOverflowed,
    TotalValueIsUnderflowed,
}

impl super::Error for Error {
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

struct GasProvider;

impl super::Provider for GasProvider {
    type ExternalOrigin = ExternalOrigin;
    type Key = MapKey;
    type ReservationKey = ReservationKey;
    type Balance = Balance;
    type InternalError = Error;
    type Error = Error;

    type GasTree = TreeImpl<
        TotalIssuanceWrap,
        Self::InternalError,
        Self::Error,
        ExternalOrigin,
        GasTreeNodesWrap,
    >;
}

type Gas = <GasProvider as super::Provider>::GasTree;

fn gas_tree_node_clone() -> BTreeMap<Key, GasNode> {
    GAS_TREE_NODES.with(|tree| {
        tree.borrow()
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect::<BTreeMap<_, _>>()
    })
}

#[derive(Debug, Default)]
struct TestTree {
    // Balance which tree is created with
    expected_balance: u64,
    // Total spent amount with `spent` procedure
    spent: u64,
    // Value caught after `consume` procedure
    caught: u64,
    // Total system reservations amount.
    system_reserve: u64,
    // Total locked amount.
    locked: u64,
}

impl TestTree {
    fn new(balance: u64) -> Self {
        Self {
            expected_balance: balance,
            ..Default::default()
        }
    }

    /// Total expenses like system reserve, locked gas, caough value, etc
    fn total_expenses(&self) -> u64 {
        let balance = self.spent + self.caught + self.system_reserve + self.locked;
        assert!(
            balance <= self.expected_balance,
            "tree has too many expenses"
        );
        balance
    }
}

#[derive(Debug)]
struct TestForest {
    trees: HashMap<GasNodeId<MapKey, ReservationKey>, TestTree>,
}

impl TestForest {
    fn create(root: MapKey, balance: u64) -> Self {
        Self {
            trees: [(root.into(), TestTree::new(balance))].into(),
        }
    }

    fn register_tree(&mut self, root: impl Into<GasNodeId<MapKey, ReservationKey>>, balance: u64) {
        let root = root.into();

        self.trees
            .entry(root)
            .and_modify(|_| unreachable!("duplicated tree: {:?}", root))
            .or_insert_with(|| TestTree::new(balance));
    }

    #[track_caller]
    fn tree_by_origin_mut(
        &mut self,
        origin: impl Into<GasNodeId<MapKey, ReservationKey>>,
    ) -> &mut TestTree {
        self.trees
            .get_mut(&origin.into())
            .expect("tree root not found")
    }

    #[track_caller]
    fn tree_mut(&mut self, node: impl Into<GasNodeId<MapKey, ReservationKey>>) -> &mut TestTree {
        let origin = Gas::get_origin_key(node).expect("child node not found");
        self.tree_by_origin_mut(origin)
    }

    fn total_expenses(&self) -> u64 {
        self.trees.values().map(|tree| tree.total_expenses()).sum()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]
    #[test]
    fn test_tree_properties((max_balance, actions) in strategies::gas_tree_props_test_strategy())
    {
        TotalIssuanceWrap::kill();
        <GasTreeNodesWrap as storage::MapStorage>::clear();

        let external = ExternalOrigin(MapKey::random());
        // `actions` can consist only from tree splits. Then it's length will
        // represent a potential amount of nodes in the tree.
        // +1 for the root
        let mut node_ids = Vec::with_capacity(actions.len() + 1);
        let root_node = MapKey::random();
        let mut forest = TestForest::create(root_node, max_balance);
        node_ids.push(root_node.into());

        // Only root has a max balance
        Gas::create(external, root_node, max_balance).expect("Failed to create gas tree");
        assert_eq!(Gas::total_supply(), max_balance);

        // Nodes on which `consume` was called
        let mut marked_consumed = BTreeSet::new();
        // Nodes that were created with `split` procedure
        let mut unspec_ref_nodes = BTreeSet::new();
        // Nodes that were created with `split_with_value` procedure
        let mut spec_ref_nodes = BTreeSet::new();
        // Nodes that were created with `reserve` procedure
        let mut reserved_nodes = BTreeSet::new();
        // Nodes on which `lock` was called
        let mut locked_nodes = BTreeSet::new();
        // Nodes on which `system_reserve` was called
        let mut system_reserve_nodes = BTreeSet::new();

        for action in actions {
            // `Error::<T>::NodeNotFound` can't occur, because of `ring_get` approach
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    let &parent = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(parent_idx);
                    let child = MapKey::random();

                    if let Err(e) = Gas::split_with_value(parent, child, amount) {
                        assertions::assert_not_invariant_error(e);
                    } else {
                        spec_ref_nodes.insert(child);
                        node_ids.push(child.into());
                    }

                }
                GasTreeAction::Split(parent_idx) => {
                    let &parent = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(parent_idx);
                    let child = MapKey::random();

                    if let Err(e) = Gas::split(parent, child) {
                        assertions::assert_not_invariant_error(e);
                    } else {
                        unspec_ref_nodes.insert(child);
                        node_ids.push(child.into());
                    }
                }
                GasTreeAction::Spend(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);

                    if let GasNodeId::Node(from) = from {
                        let res = Gas::spend(from, amount);
                        if let Err(e) = &res {
                            assertions::assert_not_invariant_error(*e);
                            // The only one possible valid error, because other ones signal about invariant problems.
                            assert_err!(res, Error::InsufficientBalance);
                        } else {
                            assert_ok!(res);
                            forest.tree_mut(from).spent += amount;
                        }
                    }
                }
                GasTreeAction::Consume(id) => {
                    let &consuming = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(id);
                    let origin = Gas::get_origin_key(consuming).expect("node exists");
                    match utils::consume_node(consuming) {
                        Ok((maybe_caught, remaining_nodes, removed_nodes)) => {
                            marked_consumed.insert(consuming);

                            // Update ids
                            node_ids.retain(|id| !removed_nodes.contains_key(id));

                            // Self check
                            {
                                let mut expected_remaining_ids = remaining_nodes.keys().copied().collect::<Vec<_>>();
                                expected_remaining_ids.sort();

                                let mut actual_remaining_ids = node_ids.clone();
                                actual_remaining_ids.sort();

                                assert_eq!(
                                    expected_remaining_ids,
                                    actual_remaining_ids
                                );
                            }

                            assertions::assert_removed_nodes_props(
                                consuming,
                                removed_nodes,
                                &remaining_nodes,
                                &marked_consumed,
                            );
                            if origin == consuming {
                                assertions::assert_root_children_removed(origin, &remaining_nodes);
                            }

                            forest.tree_by_origin_mut(origin).caught += maybe_caught.unwrap_or_default();
                        }
                        Err(e) => {
                            match e {
                                Error::NodeWasConsumed => {
                                    // double consume has happened
                                    assert!(marked_consumed.contains(&consuming));
                                    assertions::assert_not_invariant_error(e);
                                }
                                Error::ConsumedWithLock => {
                                    assert!(locked_nodes.contains(&consuming));
                                    assertions::assert_not_invariant_error(e);
                                }
                                Error::ConsumedWithSystemReservation if matches!(consuming, GasNodeId::Node(_)) => {
                                    assert!(system_reserve_nodes.contains(&consuming.to_node_id().unwrap()));
                                    assertions::assert_not_invariant_error(e);
                                }
                                _ => panic!("consumed with unknown error: {:?}", e)
                            }
                        }
                    }
                }
                GasTreeAction::Cut(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);
                    let child = MapKey::random();

                    if let Err(e) = Gas::cut(from, child, amount) {
                        assertions::assert_not_invariant_error(e)
                    } else {
                        node_ids.push(child.into());
                        forest.register_tree(child, amount);
                    }
                }
                GasTreeAction::Reserve(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);
                    let child = ReservationKey::random();


                    if let GasNodeId::Node(from) = from {
                        if let Err(e) = Gas::reserve(from, child, amount) {
                            assertions::assert_not_invariant_error(e)
                        } else {
                            node_ids.push(child.into());
                            reserved_nodes.insert(child);
                            forest.register_tree(child, amount);
                        }
                    }
                }
                GasTreeAction::Lock(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);

                    if let Err(e) = Gas::lock(from, amount) {
                        assertions::assert_not_invariant_error(e)
                    } else {
                        forest.tree_mut(from).locked += amount;
                        locked_nodes.insert(from);
                    }
                }
                GasTreeAction::Unlock(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);

                    if let Err(e) = Gas::unlock(from, amount) {
                        assertions::assert_not_invariant_error(e)
                    } else {
                        forest.tree_mut(from).locked -= amount;
                        locked_nodes.insert(from);
                    }
                }
                GasTreeAction::SystemReserve(from, amount) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);

                    if let GasNodeId::Node(from) = from {
                        if let Err(e) = Gas::system_reserve(from, amount) {
                            assertions::assert_not_invariant_error(e)
                        } else {
                            forest.tree_mut(from).system_reserve += amount;
                            system_reserve_nodes.insert(from);
                        }
                    }
                }
                GasTreeAction::SystemUnreserve(from) => {
                    let &from = NonEmpty::from_slice(&node_ids).expect("always has a tree root").ring_get(from);

                    if let GasNodeId::Node(from) = from {
                        match Gas::system_unreserve(from) {
                            Ok(amount) => {
                                forest.tree_mut(from).system_reserve -= amount;
                                system_reserve_nodes.remove(&from);
                            },
                            Err(e) => {
                                assertions::assert_not_invariant_error(e);
                            }
                        }
                    }
                }
            }

            if node_ids.is_empty() {
                // Nodes were consumed, no need to process `actions` anymore
                break;
            }
        }

        let gas_tree_ids = BTreeSet::from_iter(gas_tree_node_clone().keys().copied());

        // Self check, that in-memory view on gas tree ids is the same as persistent view.
        assert_eq!(gas_tree_ids, BTreeSet::from_iter(node_ids));

        let mut rest_value = 0;
        for (node_id, node) in gas_tree_node_clone() {
            // All nodes from one tree (forest) have the same origin
            assert_ok!(Gas::get_external(node_id), external);

            if let Some(value) = node.value() {
                rest_value += value;
            }

            // Check property: all existing specified and unspecified nodes have a parent in a tree
            if let GasNode::SpecifiedLocal { parent, .. } | GasNode::UnspecifiedLocal { parent, .. } = node {
                assert!(gas_tree_ids.contains(&parent));
                // All nodes with parent point to a parent with value
                let parent_node = GasTreeNodesWrap::get(&parent).expect("checked");
                assert!(parent_node.value().is_some());
            }

            // Check property: specified local nodes are created only with `split_with_value` call
            if node.is_specified_local() {
                assert!(spec_ref_nodes.contains(&node_id.to_node_id().unwrap()));
            } else if node.is_unspecified_local() {
                // Check property: unspecified local nodes are created only with `split` call
                assert!(unspec_ref_nodes.contains(&node_id.to_node_id().unwrap()));
            }

            // Check property: for all the nodes with system reservation currently existing in the tree...
            if node.system_reserve().map(|x| x != 0).unwrap_or(false) {
                // ...is not consumed
                assert!(!node.is_consumed());
                // ...can be with system reservation only after `system_reserve`
                assert!(system_reserve_nodes.contains(&node_id.to_node_id().unwrap()));
                // ...there can't be any existing system reserved cut and reserved nodes, because
                // cut is used for mailbox and reserved is used for signals which can't create system reservations
                assert!(node.is_external() || node.is_specified_local() || node.is_unspecified_local());
            }

            // Check property: for all the nodes with lock currently existing in the tree...
            if node.lock() != 0 {
                // ...is not consumed
                assert!(!node.is_consumed());
                // ...can be with lock only after `lock`
                assert!(locked_nodes.contains(&node_id));
            }

            // Check property: for all the `Reserved` nodes currently existing in the tree...
            if node.is_reserved() {
                let node_id = node_id.to_reservation_id().unwrap();
                // ...can exist only after `reserve`
                assert!(reserved_nodes.contains(&node_id));
            }

            // Check property: for all the consumed nodes currently existing in the tree...
            if node.is_consumed() {
                // ...have no locked value
                assert!(matches!(node.lock(), 0));
                // ..have no system reserved value
                assert!(matches!(node.system_reserve(), Some(0) | None));
                // ...existing consumed node can't have zero refs. Otherwise it must have been deleted from the storage
                assert!(node.refs() != 0);
                // ...can become consumed only after consume call (so can be deleted by intentional call, not automatically)
                assert!(marked_consumed.contains(&node_id));
                // ...there can't be any existing consumed unspecified local nodes, because they are immediately removed after the call
                assert!(node.is_external() || node.is_specified_local() || node.is_reserved());
                // ...existing consumed node with no unspec children has 0 inner value.
                // That's because anytime node becomes consumed without unspec children, it's no longer a patron.
                // So `consume` call on non-patron leads a value to be moved upstream or returned to the `origin`.
                if node.unspec_refs() == 0 {
                    let value = node.value().expect("node with value, checked");
                    assert!(value == 0);
                }
            } else {
                // If is not consumed, then no consume calls should have been called on it
                assert!(!marked_consumed.contains(&node_id));
            }

            // Check property: if node has non-zero value, it's a patron node (either not consumed or with unspec refs)
            // (Actually, patron can have 0 inner value, when `spend` decreased it's balance to 0, but it's an edge case)
            // `Cut` node can be not consumed with non zero value, but is not a patron
            if let Some(value) = node.value() {
                if value != 0 && !node.is_cut() {
                    assert!(node.is_patron());
                }
            }

            // Check property: all nodes have ancestor (node is a self-ancestor too) with value
            let (ancestor_with_value, ancestor_id) = Gas::node_with_value(node.clone()).expect("tree is invalidated");
            // The ancestor with value is either the node itself or its parent
            if ancestor_with_value != node {
                assert_eq!(node.parent(), ancestor_id);
            }
            assert!(ancestor_with_value.value().is_some());
        }

        if !gas_tree_ids.is_empty() {
            // Check trees imbalance
            assert_eq!(max_balance, rest_value + forest.total_expenses());
        }
    }

    #[test]
    fn test_empty_tree(actions in strategies::gas_tree_action_strategy(100)) {
        TotalIssuanceWrap::kill();
        GasTreeNodesWrap::clear();

        // Tree can be created only with external root

        let mut nodes = Vec::with_capacity(actions.len());

        for node in &mut nodes {
            *node = MapKey::random();
        }

        for action in actions {
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    if let Some(non_empty_nodes) = NonEmpty::from_slice(&nodes) {
                        let &parent = non_empty_nodes.ring_get(parent_idx);
                        let child = MapKey::random();

                        Gas::split_with_value(parent, child, amount).expect("Failed to split with value");
                    }
                }
                GasTreeAction::Split(parent_idx) => {
                    if let Some(non_empty_nodes) = NonEmpty::from_slice(&nodes) {
                        let &parent = non_empty_nodes.ring_get(parent_idx);
                        let child = MapKey::random();

                        Gas::split(parent, child).expect("Failed to split without value");
                    }
                }
                GasTreeAction::Reserve(parent_idx, amount) => {
                    if let Some(non_empty_nodes) = NonEmpty::from_slice(&nodes) {
                        let &parent = non_empty_nodes.ring_get(parent_idx);
                        let child = ReservationKey::random();

                        Gas::reserve(parent, child, amount).expect("Failed to create reservation");
                    }
                }
                _ => {}
            }
        }

        assert!(GAS_TREE_NODES.with(|tree| tree.borrow().iter().count()) == 0);
    }
}
