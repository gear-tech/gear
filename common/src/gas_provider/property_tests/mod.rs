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
//! 1. All non-external nodes have a parent in GasTree storage.
//! 2. All non-external nodes have at least one ancestor with value (i.e., ValueNode::node_with_value procedure always return Ok),
//! however this value can be equal to 0. Also there are no guarantees that this ancestor is a parent.
//! 3. All nodes can't have consumed parent with zero refs (there can't be any nodes like that in storage) between calls to `ValueTree::consume`.
//! Therefore, if node is deleted, it is consumed and has zero refs (and zero value, if it is able to hold value and is of `ValueType::SpecifiedLocal` type).
//! So if there is an existing consumed node, it has non-zero refs counter and a value >= 0 (between calls to `ValueTree::consume`)
//! 4. If a sub-tree of `GasTree` is a tree, where root has `ValueType::External` type, then sub-tree's root is always deleted last.
//! 5. Nodes can become consumed only after `ValueTree::consume` call.
//! 6. Unspec refs counter for the current node is incremented only after `ValueTree::split` which creates a node with `ValueType::UnspecifiedLocal` type.
//! 7. Spec refs counter for the current node is incremented only after `ValueTree::split_with_value`, which creates a node with `ValueType::SpecifiedLocal` type is.

use super::*;
use core::{cell::RefCell, iter::FromIterator, ops::DerefMut};
use frame_support::{assert_ok, traits::ConstU64};
use primitive_types::H256;
use proptest::prelude::*;
use strategies::GasTreeAction;
use utils::RingGet;

mod assertions;
mod strategies;
mod utils;

type Balance = u64;

#[thread_local]
static TOTAL_ISSUANCE: RefCell<Option<Balance>> = RefCell::new(None);

#[derive(Debug, PartialEq, Eq)]
struct TotalIssuanceWrap;

impl ValueStorage for TotalIssuanceWrap {
    type Value = Balance;

    fn exists() -> bool {
        TOTAL_ISSUANCE.borrow().is_some()
    }

    fn get() -> Option<Self::Value> {
        *TOTAL_ISSUANCE.borrow()
    }

    fn kill() {
        *TOTAL_ISSUANCE.borrow_mut() = None
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R {
        f(TOTAL_ISSUANCE.borrow_mut().deref_mut())
    }

    fn put(value: Self::Value) {
        TOTAL_ISSUANCE.replace(Some(value));
    }

    fn set(value: Self::Value) -> Option<Self::Value> {
        Self::mutate(|opt| {
            let prev = opt.take();
            *opt = Some(value);
            prev
        })
    }

    fn take() -> Option<Self::Value> {
        TOTAL_ISSUANCE.take()
    }
}

type Key = H256;
type ExternalOrigin = H256;
type GasNode = super::GasNode<ExternalOrigin, Key, Balance>;

#[thread_local]
static GAS_TREE_NODES: RefCell<BTreeMap<Key, GasNode>> = RefCell::new(BTreeMap::new());

struct GasTreeNodesWrap;

impl storage::MapStorage for GasTreeNodesWrap {
    type Key = Key;
    type Value = GasNode;

    fn contains_key(key: &Self::Key) -> bool {
        GAS_TREE_NODES.borrow().contains_key(key)
    }

    fn get(key: &Self::Key) -> Option<Self::Value> {
        GAS_TREE_NODES.borrow().get(key).map(Clone::clone)
    }

    fn insert(key: Self::Key, value: Self::Value) {
        GAS_TREE_NODES.borrow_mut().insert(key, value);
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(_key: Self::Key, _f: F) -> R {
        unimplemented!()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut _f: F) {
        unimplemented!()
    }

    fn remove(key: Self::Key) {
        GAS_TREE_NODES.borrow_mut().remove(&key);
    }

    fn clear() {
        GAS_TREE_NODES.borrow_mut().clear()
    }

    fn take(key: Self::Key) -> Option<Self::Value> {
        GAS_TREE_NODES.borrow_mut().remove(&key)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Error {
    NodeAlreadyExists,
    ParentIsLost,
    ParentHasNoChildren,
    NodeNotFound,
    NodeWasConsumed,
    InsufficientBalance,
    Forbidden,
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
}

struct GasAllowance;

impl super::storage::Limiter for GasAllowance {
    type Value = Balance;

    fn get() -> Self::Value {
        0
    }

    fn put(_gas: Self::Value) {
        unimplemented!()
    }

    fn decrease(_gas: Self::Value) {
        unimplemented!()
    }
}

struct GasProvider;

impl super::Provider for GasProvider {
    type BlockGasLimit = ConstU64<1_000>;
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

    type GasAllowance = GasAllowance;
}

type Gas = <GasProvider as super::Provider>::GasTree;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]
    #[test]
    fn test_tree_properties((max_balance, actions) in strategies::gas_tree_props_test_strategy())
    {
        TotalIssuanceWrap::kill();
        <GasTreeNodesWrap as storage::MapStorage>::clear();

        // `actions` can consist only from tree splits. Then it's length will
        // represent a potential amount of nodes in the tree.
        // +1 for the root
        let mut node_ids = Vec::with_capacity(actions.len() + 1);
        let root_node = H256::random();
        node_ids.push(root_node);

        // Only root has a max balance
        assert_ok!(Gas::create(H256::random(), root_node, max_balance));

        // Nodes on which `consume` was called
        let mut marked_consumed = BTreeSet::new();
        // Nodes that were created with `split` procedure
        let mut unspec_ref_nodes = BTreeSet::new();
        // Nodes that were created with `split_with_value` procedure
        let mut spec_ref_nodes = BTreeSet::new();
        // Total spent amount with `spent` procedure
        let mut spent = 0;

        for action in actions {
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration there is at least 1 element; qed");
                    let child = H256::random();

                    if Gas::split_with_value(parent, child, amount).is_ok() {
                        spec_ref_nodes.insert(child);
                        node_ids.push(child)
                    }
                }
                GasTreeAction::Split(parent_idx) => {
                    let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration there is at least 1 element; qed");
                    let child = H256::random();

                    if Gas::split(parent, child).is_ok() {
                        unspec_ref_nodes.insert(child);
                        node_ids.push(child);
                    }
                }
                GasTreeAction::Spend(from, amount) => {
                    let from = node_ids.ring_get(from).copied().expect("before each iteration there is at least 1 element; qed");
                    let limit = Gas::get_limit(from).unwrap().map(|(g, _)| g).unwrap();
                    let res = Gas::spend(from, amount);

                    if limit < amount {
                        assert_eq!(res, Err(Error::InsufficientBalance));
                    } else {
                        assert_ok!(res);
                        spent += amount;
                    }
                }
                GasTreeAction::Consume(id) => {
                    let consuming = node_ids.ring_get(id).copied().expect("before each iteration there is at least 1 element; qed");
                    match utils::consume_node(consuming) {
                        Ok(removed_nodes) => {
                            marked_consumed.insert(consuming);
                            // Update ids
                            node_ids.retain(|id| !removed_nodes.contains_key(id));

                            // Search operation in a set is faster, then in a vector
                            let remaining_ids = BTreeSet::from_iter(node_ids.iter().copied());
                            assertions::assert_removed_nodes_props(
                                consuming,
                                removed_nodes,
                                &remaining_ids,
                                &marked_consumed,
                            );
                            assertions::assert_root_removed_last(root_node, &remaining_ids);
                        }
                        Err(e) => {
                            // double consume has happened
                            assert!(marked_consumed.contains(&consuming));
                            assert_eq!(e, Error::NodeWasConsumed);
                        }
                    }
                }
            }

            if node_ids.is_empty() {
                // Nodes were consumed, no need to process `actions` anymore
                break;
            }
        }

        let gas_tree_ids = BTreeSet::from_iter(GAS_TREE_NODES.borrow().iter().map(|(k, _)| *k));

        // Self check, that in-memory view on gas tree ids is the same as persistent view.
        assert_eq!(gas_tree_ids, BTreeSet::from_iter(node_ids));

        let mut rest_value = 0;
        for (node_id, node) in GAS_TREE_NODES.borrow().iter() {
            if let Some(value) = node.inner_value() {
                rest_value += value;
            }

            // Check property: all nodes have parents
            if let Some(parent) = node.parent() {
                assert!(gas_tree_ids.contains(&parent));
            }

            // Check property: specified local nodes are created only with `split_with_value` call
            if matches!(node.inner, GasNodeType::SpecifiedLocal { .. }) {
                assert!(spec_ref_nodes.contains(node_id));
            } else if matches!(node.inner, GasNodeType::UnspecifiedLocal { .. }) {
                // Check property: unspecified local nodes are created only with `split` call
                assert!(unspec_ref_nodes.contains(node_id));
            }

            // Check property: for all the consumed nodes currently existing in the tree...
            if node.consumed {
                // ...existing consumed node can't have zero refs. Otherwise it must have been deleted from the storage
                assert!(node.refs() != 0);
                // ...can become consumed only after consume call (so can be deleted by intentional call, not automatically)
                assert!(marked_consumed.contains(node_id));
            } else {
                // If is not consumed, then no consume calls should have been called on it
                assert!(!marked_consumed.contains(node_id));
            }

            // Check property: all nodes have ancestor (node is a self-ancestor too) with value
            let (ancestor_with_value, _) = Gas::node_with_value(node.clone()).expect("tree is invalidated");
            assert!(ancestor_with_value.inner_value().is_some());
        }

        if !gas_tree_ids.is_empty() {
            // Check trees imbalance
            assert!(max_balance == spent + rest_value)
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

        assert!(GAS_TREE_NODES.borrow().iter().count() == 0);
    }
}
