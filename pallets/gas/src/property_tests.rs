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
use std::collections::{BTreeMap, BTreeSet};
use std::iter::FromIterator;

type Gas = Pallet<Test>;

const MAX_ACTIONS: usize = 1000;

#[derive(Debug, Clone, Copy)]
enum GasTreeAction {
    Split(usize),
    SplitWithValue(usize, u64),
    Spend(usize, u64),
    Consume(usize),
}

fn gas_action_strategy(max_balance: u64) -> impl Strategy<Value = Vec<GasTreeAction>> {
    let action_random_variant = prop_oneof![
        (any::<usize>(), 0..max_balance).prop_flat_map(|(id, amount)| {
            prop_oneof![
                Just(GasTreeAction::SplitWithValue(id, amount)),
                Just(GasTreeAction::Spend(id, amount))
            ]
        }),
        any::<usize>().prop_flat_map(|id| {
            prop_oneof![
                Just(GasTreeAction::Consume(id)),
                Just(GasTreeAction::Split(id))
            ]
        }),
    ];
    prop::collection::vec(action_random_variant, 0..MAX_ACTIONS)
}

// todo [sab] потом сделай более абстрактным
trait RingGet<T> {
    fn ring_get(&self, index: usize) -> Option<&T>;
}

impl<T> RingGet<T> for Vec<T> {
    fn ring_get(&self, index: usize) -> Option<&T> {
        let is_not_empty = !self.is_empty();
        is_not_empty
            .then(|| index % self.len())
            .and_then(|idx| self.get(idx))
    }
}

fn consume_node(consuming: H256) -> Result<BTreeMap<H256, ValueNode>, ()> {
    let nodes_before_consume = BTreeMap::from_iter(super::GasTree::<Test>::iter());
    Gas::consume(consuming)
        .and_then(|_| {
            let nodes_after_consume = BTreeSet::from_iter(super::GasTree::<Test>::iter_keys());
            let mut removed_nodes = BTreeMap::new();
            for (id, node) in nodes_before_consume {
                if !nodes_after_consume.contains(&id) {
                    // was removed
                    removed_nodes.insert(id, node);
                }
            }

            Ok(removed_nodes)
        })
        .map_err(|_| ())
}

fn assert_removed_nodes_props(
    consumed: H256,
    removed_nodes: BTreeMap<H256, ValueNode>,
    remaining_ids: &BTreeSet<H256>,
    marked_consumed_nodes: &BTreeSet<H256>,
) {
    if removed_nodes.is_empty() {
        return;
    }

    assert_removed_nodes_are_consumed(consumed, &marked_consumed_nodes, &removed_nodes);
    assert_removed_nodes_form_path(consumed, &remaining_ids, removed_nodes);
}

fn assert_removed_nodes_are_consumed(
    consumed: H256,
    marked_consumed_nodes: &BTreeSet<H256>,
    removed_nodes: &BTreeMap<H256, ValueNode>,
) {
    for (id, node) in removed_nodes {
        if *id != consumed {
            assert!(node.consumed);
            assert!(node.refs() == 1);
        } else {
            // todo [sab] set consumed
            assert!(node.refs() == 0);
        }

        // Were explicitly consumed, not automatically
        assert!(marked_consumed_nodes.contains(id))
    }
}

fn assert_removed_nodes_form_path(
    consumed: H256,
    remaining_ids: &BTreeSet<H256>,
    removed_nodes: BTreeMap<H256, ValueNode>,
) {
    let mut not_checked_parents_count = removed_nodes.len();
    let mut node = removed_nodes
        .get(&consumed)
        .expect("consumed node is absent in removed nodes map");

    while not_checked_parents_count > 1 {
        if let Some(parent) = node.parent() {
            assert!(!remaining_ids.contains(&parent));
            assert!(removed_nodes.contains_key(&parent));

            not_checked_parents_count -= 1;
            node = removed_nodes.get(&parent).expect("checked");
        }
    }
    if let Some(parent) = node.parent() {
        assert!(remaining_ids.contains(&parent));
    }
}

fn assert_root_removed_last(root_node: H256, remaining_ids: &BTreeSet<H256>) {
    // Check root is always deleted in the last consume call for the current gas tree,
    // i.e., if root deleted, no more nodes are in a tree.
    if Gas::get_node(&root_node).is_none() {
        assert!(remaining_ids.is_empty());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]
    #[test]
    fn test_tree_properties((max_tree_node_balance, actions) in any::<u64>().prop_flat_map(|max_balance| {
        (Just(max_balance), gas_action_strategy(max_balance))
    })) {
        new_test_ext().execute_with(|| {
            // Test some of gas tree properties
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
    fn test_empty_tree(actions in gas_action_strategy(100)) {
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

