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
//! 1. Nodes can become consumed only after [`Tree::consume`] call.
//! 2. Unspec refs counter for the current node is incremented only after [`Tree::split`] which creates a node with [`GasNodeType::UnspecifiedLocal`] type.
//! 3. Spec refs counter for the current node is incremented only after [`GasNodeType::split_with_value`], which creates a node with [`GasNodeType::SpecifiedLocal`] type.
//! 4. All nodes, except for [`GasNodeType::ReservedLocal`] and [`GasNodeType::External`] have a parent in GasTree storage.
//! 5. All nodes with parent point to a parent with value. So If a `key` is an id of [`GasNodeType::SpecifiedLocal`] or [`GasNodeType::External`] node,
//! the node under this `key` will always be a parent of the newly generated node after [`Tree::split`]/[`Tree::split_with_value`] call.
//! However, there is no such guarantee if key is an id of the [`GasNodeType::UnspecifiedLocal`] nodes.
//! 6. All non-external nodes have ancestor with value (i.e., [`TreeImpl::node_with_value`] procedure always return `Ok`), however this value can be equal to 0.
//! This ancestor is either a parent or the node itself.
//! 7. All nodes can't have consumed parent with zero refs (there can't be any nodes like that in storage) between calls to [`Tree::consume`].
//! Therefore, if node is deleted, it is consumed and has zero refs (and zero value).
//! 8. [`GasNodeType::UnspecifiedLocal`] nodes are always leaves in the tree (they have no children), so they are always deleted after consume call.
//! The same ruling is for [`GasNodeType::ReservedLocal`] nodes.
//! So there can't be any [`GasNodeType::UnspecifiedLocal`] node in the tree with consumed field set to true.
//! So if there is an **existing consumed** node, then it has non-zero refs counter and a value >= 0 (between calls to [`Tree::consume`])
//! 9. In a tree a root with [`GasNodeType::External`] type is always deleted last.
//! 10. If node wasn't removed after `consume` it's [`GasNodeType::SpecifiedLocal`] or [`GasNodeType::External`] node. This is pretty same as the previous invariant,
//! but focuses more on [`Tree::consume`] procedure, while the other focuses on the all tree invariant. (checked in `consume` call assertions).
//! 11. [`GasNodeType::UnspecifiedLocal`] and [`GasNodeType::ReservedLocal`] nodes can't be removed, nor mutated during cascade removal. So after [`Tree::consume`] call not more than one node is of [`GasNodeType::UnspecifiedLocal`] type.
//! 12. Between calls to [`Tree::consume`] if node is consumed and has no unspec refs, it's internal gas value is zero.
//! 13. Between calls to [`Tree::consume`] if node has value, it's either not consumed or it has unspecified children.
//! 14. Value catch can be performed only on consumed nodes (not tested).

use super::*;
use crate::storage::MapStorage;
use core::{cell::RefCell, iter::FromIterator, ops::DerefMut};
use frame_support::assert_ok;
use primitive_types::H256;
use proptest::prelude::*;
use strategies::GasTreeAction;
use utils::RingGet;

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

type Key = H256;
type ExternalOrigin = H256;
type GasNode = super::GasNode<ExternalOrigin, Key, Balance>;

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
}

struct GasProvider;

impl super::Provider for GasProvider {
    type ExternalOrigin = ExternalOrigin;
    type Key = Key;
    type Balance = Balance;
    type PositiveImbalance = PositiveImbalance<Self::Balance, TotalIssuanceWrap>;
    type NegativeImbalance = NegativeImbalance<Self::Balance, TotalIssuanceWrap>;
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]
    #[test]
    fn test_tree_properties((max_balance, actions) in strategies::gas_tree_props_test_strategy())
    {
        TotalIssuanceWrap::kill();
        <GasTreeNodesWrap as storage::MapStorage>::clear();

        let origin = H256::random();
        // `actions` can consist only from tree splits. Then it's length will
        // represent a potential amount of nodes in the tree.
        // +1 for the root
        let mut node_ids = Vec::with_capacity(actions.len() + 1);
        let root_node = H256::random();
        node_ids.push(root_node);

        // Only root has a max balance
        let _ = Gas::create(origin, root_node, max_balance);
        assert_eq!(Gas::total_supply(), max_balance);

        // Nodes on which `consume` was called
        let mut marked_consumed = BTreeSet::new();
        // Nodes that were created with `split` procedure
        let mut unspec_ref_nodes = BTreeSet::new();
        // Nodes that were created with `split_with_value` procedure
        let mut spec_ref_nodes = BTreeSet::new();
        // Total spent amount with `spent` procedure
        let mut spent = 0;
        // Value caught after `consume` procedure
        let mut caught = 0;

        for action in actions {
            // `Error::<T>::NodeNotFound` can't occur, because of `ring_get` approach
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration there is at least 1 element; qed");
                    let child = H256::random();

                    if let Err(e) = Gas::split_with_value(parent, child, amount) {
                        assertions::assert_not_invariant_error(e);
                    } else {
                        spec_ref_nodes.insert(child);
                        node_ids.push(child)
                    }
                }
                GasTreeAction::Split(parent_idx) => {
                    let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration there is at least 1 element; qed");
                    let child = H256::random();

                    if let Err(e) = Gas::split(parent, child) {
                        assertions::assert_not_invariant_error(e);
                    } else {
                        unspec_ref_nodes.insert(child);
                        node_ids.push(child);
                    }
                }
                GasTreeAction::Spend(from, amount) => {
                    let from = node_ids.ring_get(from).copied().expect("before each iteration there is at least 1 element; qed");
                    let res = Gas::spend(from, amount);

                    if let Err(e) = &res {
                        assertions::assert_not_invariant_error(*e);
                        // The only one possible valid error, because other ones signal about invariant problems.
                        assert_eq!(res, Err(Error::InsufficientBalance));
                    } else {
                        assert_ok!(res);
                        spent += amount;
                    }
                }
                GasTreeAction::Consume(id) => {
                    let consuming = node_ids.ring_get(id).copied().expect("before each iteration there is at least 1 element; qed");
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
                            assertions::assert_root_removed_last(root_node, remaining_nodes);

                            caught += maybe_caught.unwrap_or_default();
                        }
                        Err(e) => {
                            // double consume has happened
                            assert!(marked_consumed.contains(&consuming));
                            assert_eq!(e, Error::NodeWasConsumed);

                            assertions::assert_not_invariant_error(e);
                        }
                    }
                }
                GasTreeAction::Cut(from, amount) => {
                    let from = node_ids.ring_get(from).copied().expect("before each iteration there is at least 1 element; qed");
                    let child = H256::random();

                    if let Err(e) = Gas::cut(from, child, amount) {
                        assertions::assert_not_invariant_error(e)
                    } else {
                        node_ids.push(child);
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
            assert_eq!(
                Gas::get_origin(node_id)
                    .map(|maybe_origin| maybe_origin.map(|(_, origin)| origin)),
                Ok(Some(origin))
            );

            if let Some(value) = node.inner_value() {
                rest_value += value;
            }

            // Check property: all existing specified and unspecified nodes have a parent in a tree
            if let GasNodeType::SpecifiedLocal { parent, .. } | GasNodeType::UnspecifiedLocal { parent } = node.inner {
                assert!(gas_tree_ids.contains(&parent));
                // All nodes with parent point to a parent with value
                let parent_node = GasTreeNodesWrap::get(&parent).expect("checked");
                assert!(parent_node.inner_value().is_some());
            }

            // Check property: specified local nodes are created only with `split_with_value` call
            if matches!(node.inner, GasNodeType::SpecifiedLocal { .. }) {
                assert!(spec_ref_nodes.contains(&node_id));
            } else if matches!(node.inner, GasNodeType::UnspecifiedLocal { .. }) {
                // Check property: unspecified local nodes are created only with `split` call
                assert!(unspec_ref_nodes.contains(&node_id));
            }

            // Check property: for all the consumed nodes currently existing in the tree...
            if node.consumed {
                // ...existing consumed node can't have zero refs. Otherwise it must have been deleted from the storage
                assert!(node.refs() != 0);
                // ...can become consumed only after consume call (so can be deleted by intentional call, not automatically)
                assert!(marked_consumed.contains(&node_id));
                // ...there can't be any existing consumed unspecified local nodes, because they are immediately removed after the call
                assert!(node.inner.is_external() || node.inner.is_specified_local());
                // ...existing consumed node with no unspec children has 0 inner value.
                // That's because anytime node becomes consumed without unspec children, it's no longer a patron.
                // So `consume` call on non-patron leads a value to be moved upstream or returned to the `origin`.
                if node.unspec_refs() == 0 {
                    let value = node.inner_value().expect("node with value, checked");
                    assert!(value == 0);
                }
            } else {
                // If is not consumed, then no consume calls should have been called on it
                assert!(!marked_consumed.contains(&node_id));
            }

            // Check property: if node has non-zero value, it's a patron node (either not consumed or with unspec refs)
            // (Actually, patron can have 0 inner value, when `spend` decreased it's balance to 0, but it's an edge case)
            // ReservedLocal node can be not consumed with non zero value, but is not a patron
            if let Some(value) = node.inner_value() {
                if value != 0 && !node.inner.is_reserved_local() {
                    assert!(node.is_patron());
                }
            }

            // Check property: all nodes have ancestor (node is a self-ancestor too) with value
            let (ancestor_with_value, ancestor_id) = Gas::node_with_value(node.clone()).expect("tree is invalidated");
            // The ancestor with value is either the node itself or its parent
            if ancestor_with_value != node {
                assert_eq!(node.parent(), ancestor_id);
            }
            assert!(ancestor_with_value.inner_value().is_some());
        }

        if !gas_tree_ids.is_empty() {
            // Check trees imbalance
            assert!(max_balance == spent + rest_value + caught)
        }
    }

    #[test]
    fn test_empty_tree(actions in strategies::gas_tree_action_strategy(100)) {
        TotalIssuanceWrap::kill();
        <GasTreeNodesWrap as storage::MapStorage>::clear();

        // Tree can be created only with external root

        let mut nodes = Vec::with_capacity(actions.len());
        for node in &mut nodes {
            *node = H256::random();
        }

        for action in actions {
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    if let Some(&parent) = nodes.ring_get(parent_idx) {
                        let child = H256::random();

                        let _ = Gas::split_with_value(parent, child, amount);
                    }
                }
                GasTreeAction::Split(parent_idx) => {
                    if let Some(&parent) = nodes.ring_get(parent_idx) {
                        let child = H256::random();

                        let _ = Gas::split(parent, child);
                    }
                }
                _ => {}
            }
        }

        assert!(GAS_TREE_NODES.with(|tree| tree.borrow().iter().count()) == 0);
    }
}
