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
use crate::mock::*;
use frame_support::assert_ok;
use primitive_types::H256;
use proptest::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter::FromIterator,
};
use strategies::GasTreeAction;
use utils::RingGet;

mod assertions;
mod strategies;
mod utils;

type Gas = Pallet<Test>;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]
    #[test]
    fn test_tree_properties((max_balance, actions) in strategies::gas_tree_props_test_strategy())
    {
        new_test_ext().execute_with(|| {
            // `actions` can consist only from tree splits. Then it's length will
            // represent a potential amount of nodes in the tree.
            let mut node_ids = Vec::with_capacity(actions.len() + 1); // +1 for the root
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
                        let limit = Gas::get_limit(from).unwrap().unwrap();
                        let res = Gas::spend(from, amount);

                        if limit < amount {
                            assert_eq!(res, Err(Error::<Test>::InsufficientBalance.into()));
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
                                assert_eq!(e, Error::<Test>::NodeWasConsumed.into());
                            }
                        }
                    }
                }

                if node_ids.is_empty() {
                    // Nodes were consumed, no need to process `actions` anymore
                    break;
                }
            }

            let gas_tree_ids = BTreeSet::from_iter(GasTree::<Test>::iter_keys());

            // Self check, that in-memory view on gas tree ids is the same as persistent view.
            assert_eq!(gas_tree_ids, BTreeSet::from_iter(node_ids));

            let mut rest_value = 0;
            for node in GasTree::<Test>::iter_values() {
                if let Some(value) = node.inner_value() {
                    rest_value += value;
                }

                // Check property: all nodes have parents
                if let Some(parent) = node.parent() {
                    assert!(gas_tree_ids.contains(&parent));
                }

                // Check property: specified local nodes are created only with `split_with_value` call
                if matches!(node.inner, ValueType::SpecifiedLocal { .. }) {
                    assert!(spec_ref_nodes.contains(&node.id));
                } else if matches!(node.inner, ValueType::UnspecifiedLocal { .. }) {
                    // Check property: unspecified local nodes are created only with `split` call
                    assert!(unspec_ref_nodes.contains(&node.id));
                }

                // Check property: for all the consumed nodes currently existing in the tree...
                if node.consumed {
                    // ...existing consumed node can't have zero refs. Otherwise it must have been deleted from the storage
                    assert!(node.refs() != 0);
                    // ...can become consumed only after consume call (so can be deleted by intentional call, not automatically)
                    assert!(marked_consumed.contains(&node.id));
                } else {
                    // If is not consumed, then no consume calls should have been called on it
                    assert!(!marked_consumed.contains(&node.id));
                }

                // Check property: all nodes have ancestor (node is a self-ancestor too) with value
                let ancestor_with_value = node.node_with_value::<Test>().expect("tree is invalidated");
                assert!(ancestor_with_value.inner_value().is_some());
            }

            if !gas_tree_ids.is_empty() {
                // Check trees imbalance
                assert!(max_balance == spent + rest_value)
            }
        })
    }

    #[test]
    fn test_empty_tree(actions in strategies::gas_tree_action_strategy(100)) {
        new_test_ext().execute_with(|| {
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

            assert!(GasTree::<Test>::iter_values().count() == 0);
        })
    }
}
