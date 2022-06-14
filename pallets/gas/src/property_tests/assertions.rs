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

pub(super) fn assert_removed_nodes_props(
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

pub(super) fn assert_removed_nodes_are_consumed(
    consumed: H256,
    marked_consumed_nodes: &BTreeSet<H256>,
    removed_nodes: &BTreeMap<H256, ValueNode>,
) {
    for (id, node) in removed_nodes {
        if *id != consumed {
            assert!(node.consumed);
            assert!(node.refs() == 1);
        } else {
            assert!(node.refs() == 0);
        }

        // Were explicitly consumed, not automatically
        assert!(marked_consumed_nodes.contains(id))
    }
}

pub(super) fn assert_removed_nodes_form_path(
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

pub(super) fn assert_root_removed_last(root_node: H256, remaining_ids: &BTreeSet<H256>) {
    // Check root is always deleted in the last consume call for the current gas tree,
    // i.e., if root deleted, no more nodes are in a tree.
    if Gas::get_node(&root_node).is_none() {
        assert!(remaining_ids.is_empty());
    }
}
