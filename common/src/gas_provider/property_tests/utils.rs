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

pub type MaybeCaughtValue = Option<u64>;
pub type RemainingNodes = BTreeMap<Key, GasNode>;
pub type RemovedNodes = BTreeMap<Key, GasNode>;

/// Consumes node with `consuming` id and returns a map of removed nodes
pub(super) fn consume_node(
    consuming: Key,
) -> Result<(MaybeCaughtValue, RemainingNodes, RemovedNodes), super::Error> {
    let nodes_before_consume = gas_tree_node_clone();
    Gas::consume(consuming).map(|maybe_output| {
        let maybe_caught_value = maybe_output.map(|(imb, _)| imb.peek());
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
