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
use crate::mock::*;
use frame_support::assert_ok;
use primitive_types::H256;
use proptest::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter::FromIterator,
};

use assertions::*;
use strategies::*;
use utils::*;

mod assertions;
mod strategies;
mod utils;

type Gas = Pallet<Test>;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]
    #[test]
    fn test_tree_properties((max_tree_node_balance, actions) in gas_tree_props_test_strategy())
    {
        new_test_ext().execute_with(|| {
            let mut node_ids = Vec::with_capacity(actions.len());
            let root_node = H256::random();
            node_ids.push(root_node);

            assert_ok!(Gas::create(H256::random(), root_node, max_tree_node_balance));

            let mut marked_consumed = BTreeSet::new();
            let mut unspec_ref_nodes = BTreeSet::new();
            let mut spec_ref_nodes = BTreeSet::new();
            let mut spent = 0;
            for action in actions {
                match action {
                    GasTreeAction::SplitWithValue(parent_idx, amount) => {
                        let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration has at least 1 element; qed");
                        let child = H256::random();

                        if Gas::split_with_value(parent, child, amount).is_ok() {
                            spec_ref_nodes.insert(child);
                            node_ids.push(child)
                        }
                    }
                    GasTreeAction::Split(parent_idx) => {
                        let parent = node_ids.ring_get(parent_idx).copied().expect("before each iteration has at least 1 element; qed");
                        let child = H256::random();

                        if Gas::split(parent, child).is_ok() {
                            unspec_ref_nodes.insert(child);
                            node_ids.push(child);
                        }
                    }
                    GasTreeAction::Spend(from, amount) => {
                        let from = node_ids.ring_get(from).copied().expect("before each iteration has at least 1 element; qed");
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
                        let consuming = node_ids.ring_get(id).copied().expect("before each iteration has at least 1 element; qed");
                        if let Ok(removed_nodes) = consume_node(consuming) {
                            marked_consumed.insert(consuming);
                            // Update ids
                            node_ids.retain(|id| !removed_nodes.contains_key(id));

                            // Search operation in a set is faster, then in a vector
                            let remaining_ids = BTreeSet::from_iter(node_ids.iter().copied());
                            assert_removed_nodes_props(
                                consuming,
                                removed_nodes,
                                &remaining_ids,
                                &marked_consumed,
                            );
                            assert_root_removed_last(root_node, &remaining_ids);
                        }
                    }
                }

                if node_ids.is_empty() {
                    // Nodes were consumed, no need to process actions anymore
                    break;
                }
            }

            let gas_tree_ids = BTreeSet::from_iter(super::GasTree::<Test>::iter_keys());

            // Self check, that in-memory view on gas tree ids is the same as persistent view.
            assert_eq!(gas_tree_ids, BTreeSet::from_iter(node_ids));

            let mut rest_value = 0;
            for node in super::GasTree::<Test>::iter_values() {
                // All nodes have parents
                if let Some(parent) = node.parent() {
                    assert!(gas_tree_ids.contains(&parent));
                }

                if matches!(node.inner, ValueType::SpecifiedLocal { .. }) {
                    assert!(spec_ref_nodes.contains(&node.id));
                } else if matches!(node.inner, ValueType::UnspecifiedLocal { .. }) {
                    assert!(unspec_ref_nodes.contains(&node.id));
                }

                // All nodes have ancestor (node is a self-ancestor too) with value
                let ancestor_with_value = node.node_with_value::<Test>().expect("tree is invalidated");
                assert!(ancestor_with_value.inner_value().is_some());

                if node.consumed {
                    // Existing consumed node can't have zero refs. Otherwise it must have been deleted from the storage
                    assert!(node.refs() != 0);
                    // Can become consumed only after consume call (so can be deleted by intentional call, not automatically)
                    assert!(marked_consumed.contains(&node.id));
                } else {
                    // If is not consumed, then no consume calls should have been called on it
                    assert!(!marked_consumed.contains(&node.id));
                }

                if let Some(value) = node.inner_value() {
                    rest_value += value;
                }
            }

            if !gas_tree_ids.is_empty() {
                assert!(max_tree_node_balance == spent + rest_value)
            }
        })
    }

    #[test]
    fn test_empty_tree(actions in gas_tree_action_strategy(100)) {
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

            assert!(super::GasTree::<Test>::iter_values().count() == 0);
        })
    }
}
