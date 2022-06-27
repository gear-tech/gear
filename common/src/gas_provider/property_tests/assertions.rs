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

//! Module contains assertion checks that are used during property tests.

use super::*;

/// Check that removed nodes invariants are met
pub(super) fn assert_removed_nodes_props(
    consumed: Key,
    removed_nodes: BTreeMap<Key, ValueNode>,
    remaining_ids: &BTreeSet<Key>,
    marked_consumed_nodes: &BTreeSet<Key>,
) {
    if removed_nodes.is_empty() {
        return;
    }

    assert_removed_nodes_are_consumed(consumed, marked_consumed_nodes, &removed_nodes);
    assert_removed_nodes_form_path(consumed, remaining_ids, removed_nodes);
}

// Check that for all the removed nodes:
// 1. **They don't have children**.
// Actually we check that they have 1 child ref, each of which points to the node in `removed_nodes`.
// It's the same to say, that removed nodes have no refs, because `removed_nodes` data is gathered
// from the tree before calling [`ValueTree::consume`] procedure, when `removed_nodes` have at most
// 1 child ref. Obviously, it's impossible to get node's data after it was deleted to be sure it had
// 0 refs after deletion. The only node which can be checked to have 0 refs is the `consumed` one.
// 2. **They are marked consumed**.
// That is true for all the removed nodes except for the `consumed` one, because when it's removed
// it's redundant to update it's status in the persistence layer to `consumed`.
fn assert_removed_nodes_are_consumed(
    consumed: Key,
    marked_consumed_nodes: &BTreeSet<Key>,
    removed_nodes: &BTreeMap<Key, ValueNode>,
) {
    for (id, node) in removed_nodes {
        if *id != consumed {
            assert!(node.consumed);
            assert!(node.refs() == 1);
        } else {
            assert!(node.refs() == 0);
        }

        // Check that they were explicitly consumed, not automatically
        assert!(marked_consumed_nodes.contains(id))
    }
}

// Check that removed nodes form a path (if more than one was removed).
fn assert_removed_nodes_form_path(
    consumed: Key,
    remaining_ids: &BTreeSet<Key>,
    removed_nodes: BTreeMap<Key, ValueNode>,
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

// Check that `root_node` was removed the last.
// That is done the following way. Each time `consume` procedure is called we check `root_node` for existence.
// If it was removed after a new `consume` call, then all the tree must be empty. So no nodes can be removed
// after root was removed in the `consume` call.
pub(super) fn assert_root_removed_last(root_node: Key, remaining_ids: &BTreeSet<Key>) {
    if Gas::get_node(root_node).is_none() {
        assert!(remaining_ids.is_empty());
    }
}
