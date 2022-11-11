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
use utils::{RemainingNodes, RemovedNodes};

/// Check that removed nodes invariants are met
#[track_caller]
pub(super) fn assert_removed_nodes_props(
    consumed: Key,
    removed_nodes: RemovedNodes,
    remaining_nodes: &RemainingNodes,
    marked_consumed_nodes: &BTreeSet<Key>,
) {
    if removed_nodes.is_empty() {
        return;
    }
    assert_not_removed_node_type(consumed, remaining_nodes);
    assert_unspec_nodes_amount(&removed_nodes);
    assert_removed_nodes_have_no_lock(&removed_nodes);
    assert_removed_nodes_are_consumed(consumed, marked_consumed_nodes, &removed_nodes);
    assert_removed_nodes_form_path(consumed, remaining_nodes, removed_nodes);
}

// Check that if node was consumed, but not removed, it's one of `External` or
// `SpecifiedLocal` type. So not `UnspecifiedLocal` or `ReservedLocal`
fn assert_not_removed_node_type(consumed: Key, remaining_nodes: &RemainingNodes) {
    if let Some(consumed) = remaining_nodes.get(&consumed) {
        // Node was not removed after consume, so should be of specific types
        assert!(consumed.is_external() || consumed.is_specified_local());
    }
}

// Check cascade consumption can't remove unspec nodes, they are removed only
// from `consume` call, so not more than one unspec node is removed after
// `consume` call.
fn assert_unspec_nodes_amount(removed_nodes: &RemovedNodes) {
    let removed_unspec_count = removed_nodes
        .values()
        .filter(|node| node.is_unspecified_local())
        .count();
    assert!(removed_unspec_count <= 1);
}

// Check that for all the removed nodes:
// 1. **They don't have children**
//
// Actually we check that they have one child ref, each of which points to the
// node in `removed_nodes`. It's the same to say, that removed nodes have no
// refs, because `removed_nodes` data is gathered from the tree before
// calling [`ValueTree::consume`] procedure, when `removed_nodes` have
// at most one child ref.
//
// Obviously, it's impossible to get node's data after it was deleted to be sure
// it had no refs after deletion. The only node which can be checked to have no
// refs is the `consumed` one.
//
// 2. **They are marked consumed.**
//
// That is true for all the removed nodes except for the `consumed` one, because
// when it's removed it's redundant to update it's status in the persistence
// layer to `consumed`.
fn assert_removed_nodes_are_consumed(
    consumed: Key,
    marked_consumed_nodes: &BTreeSet<Key>,
    removed_nodes: &RemovedNodes,
) {
    for (id, node) in removed_nodes {
        if *id != consumed {
            assert!(node.is_consumed());
            assert!(node.refs() == 1);
        } else {
            assert!(node.refs() == 0);
        }

        // Check that they were explicitly consumed, not automatically.
        assert!(marked_consumed_nodes.contains(id))
    }
}

// Check that removed nodes have no locked value.
fn assert_removed_nodes_have_no_lock(removed_nodes: &RemovedNodes) {
    for node in removed_nodes.values() {
        let lock = node.lock();

        if !node.is_lockable() {
            assert!(lock.is_none());
        } else {
            assert_eq!(lock, Some(0));
        }
    }
}

// Check that removed nodes form a path (if more than one was removed).
#[track_caller]
fn assert_removed_nodes_form_path(
    consumed: Key,
    remaining_nodes: &RemainingNodes,
    removed_nodes: RemovedNodes,
) {
    let mut not_checked_parents_count = removed_nodes.len();
    let mut node = removed_nodes
        .get(&consumed)
        .expect("consumed node is absent in removed nodes map");

    while not_checked_parents_count > 1 {
        if let Some(parent) = node.parent() {
            assert!(!remaining_nodes.contains_key(&parent));
            assert!(removed_nodes.contains_key(&parent));

            not_checked_parents_count -= 1;
            node = removed_nodes.get(&parent).expect("checked");
        }
    }
    if let Some(parent) = node.parent() {
        assert!(remaining_nodes.contains_key(&parent));
    }
}

// Check that `root_node` was removed the last.
//
// That is done the following way: each time `consume` procedure is called we
// check `root_node` for existence. If it was removed after a new `consume`
// call, then all the tree must be empty. So no nodes can be removed after
// root was removed in the `consume` call.
#[track_caller]
pub(super) fn assert_root_children_removed(root_node: Key, remaining_nodes: &RemainingNodes) {
    let is_child = |id: GasNodeId<_, _>| {
        let (_, origin_id) = Gas::get_origin_node(id).unwrap();
        origin_id == root_node
    };

    if Gas::get_node(root_node).is_none() {
        assert_eq!(
            remaining_nodes
                .iter()
                .filter(|(id, _node)| is_child(**id))
                .map(|d| dbg!(d))
                .count(),
            0
        );
    };
}

// Check that returned dispatch error is not of invariant error variants.
#[track_caller]
pub(super) fn assert_not_invariant_error(err: super::Error) {
    use super::Error::*;

    match err {
        ParentIsLost
        | ParentHasNoChildren
        | UnexpectedConsumeOutput
        | UnexpectedNodeType
        | ValueIsNotCaught
        | ValueIsBlocked
        | ValueIsNotBlocked => panic!("Invariant error occurred {:?}", err),
        _ => log::error!("Non invariant error occurred: {:?}", err),
    }
}
