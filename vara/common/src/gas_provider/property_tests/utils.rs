// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;

pub type MaybeCaughtValue = Option<u64>;
pub type RemainingNodes = BTreeMap<NodeId, Node>;
pub type RemovedNodes = BTreeMap<NodeId, Node>;

/// Consumes node with `consuming` id and returns a map of removed nodes
pub(super) fn consume_node(
    consuming: NodeId,
) -> Result<(MaybeCaughtValue, RemainingNodes, RemovedNodes), GasTreeError> {
    let nodes_before_consume = gas_tree_node_clone();
    Gas::consume(consuming).map(|maybe_output| {
        let maybe_caught_value = maybe_output.map(|(imb, ..)| imb.peek());
        let remaining_nodes = gas_tree_node_clone();
        let mut removed_nodes = BTreeMap::new();
        for (id, node) in nodes_before_consume {
            if !remaining_nodes.contains_key(&id) {
                // was removed
                removed_nodes.insert(id, node);
            }
        }

        (maybe_caught_value, remaining_nodes, removed_nodes)
    })
}
