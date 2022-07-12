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

pub(super) trait RingGet<T> {
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

/// Consumes node with `consuming` id and returns a map of removed nodes
pub(super) fn consume_node(
    consuming: H256,
) -> Result<(MaybeCaughtValue, RemainingNodes, RemovedNodes), super::Error> {
    let nodes_before_consume = BTreeMap::from_iter(GAS_TREE_NODES.borrow().iter().map(|(k, v)| (*k, v.clone())));
    Gas::consume(consuming).map(|maybe_output| {
        let maybe_caught_value = maybe_output.map(|(imb, _)| imb.peek());
        let nodes_after_consume =
            BTreeSet::from_iter(GAS_TREE_NODES.borrow().iter().map(|(k, v)| (*k, v.clone())));
        let mut removed_nodes = BTreeMap::new();
        for (id, node) in nodes_before_consume {
            if !nodes_after_consume.contains_key(&id) {
                // was removed
                removed_nodes.insert(id, node);
            }
        }

        (maybe_caught_value, nodes_after_consume, removed_nodes)
    })
}
