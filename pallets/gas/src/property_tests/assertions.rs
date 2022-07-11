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
pub(super) fn assert_removed_nodes_props(
    consumed: H256,
    removed_nodes: RemovedNodes,
    remaining_nodes: &RemainingNodes,
    marked_consumed_nodes: &BTreeSet<H256>,
) {
    if removed_nodes.is_empty() {
        return;
    }
    assert_not_removed_node_type(consumed, &remaining_nodes);
    assert_unspec_nodes_amount(&removed_nodes);
    assert_removed_nodes_are_consumed(consumed, marked_consumed_nodes, &removed_nodes);
    assert_removed_nodes_form_path(consumed, remaining_nodes, removed_nodes);
}

// Check that if node was consumed, but not removed, it's of `SpecifiedLocal` or `External` types.
fn assert_not_removed_node_type(consumed: H256, remaining_nodes: &RemainingNodes) {
    if let Some(consumed) = remaining_nodes.get(&consumed) {
        // Node was not removed after consume, so should be of specific types
        assert!(consumed.inner.is_external() || consumed.inner.is_specified_local());
    }
}

// Check cascade consumption can't remove unspec nodes, they are removed only from `consume` call,
// so not more than one unspec node is removed after `consume` call.
fn assert_unspec_nodes_amount(removed_nodes: &RemovedNodes) {
    let removed_unspec_count = removed_nodes
        .values()
        .filter(|node| node.inner.is_unspecified_local())
        .count();
    assert!(removed_unspec_count <= 1);
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

        // Check that they were explicitly consumed, not automatically
        assert!(marked_consumed_nodes.contains(id))
    }
}

// Check that removed nodes form a path (if more than one was removed).
fn assert_removed_nodes_form_path(
    consumed: H256,
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
// That is done the following way. Each time `consume` procedure is called we check `root_node` for existence.
// If it was removed after a new `consume` call, then all the tree must be empty. So no nodes can be removed
// after root was removed in the `consume` call.
pub(super) fn assert_root_removed_last(root_node: H256, remaining_nodes: RemainingNodes) {
    if Gas::get_node(&root_node).is_none() {
        assert!(remaining_nodes.is_empty());
    }
}

// Check that returned dispatch error is not of invariant error variants
pub(super) fn assert_not_invariant_error(dispatch_err: DispatchError) {
    // todo [sab] don't like strs, maybe some other canonical way?
    if let DispatchError::Module(module_err) = dispatch_err {
        let pallet_err = module_err
            .message
            .expect("internal error: no error message");
        let has_invariant_error = matches!(
            pallet_err,
            "ParentIsLost"
                | "ParentHasNoChildren"
                | "UnexpectedConsumeOutput"
                | "UnexpectedNodeType"
                | "NodeIsNotPatron"
                | "ValueIsNotCaught"
                | "ValueIsBlocked"
                | "ValueIsNotBlocked"
        );
        if has_invariant_error {
            panic!("invariant error occurred {:?}", pallet_err)
        }
    }
}
