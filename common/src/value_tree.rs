// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! This presents how some finite value maybe split over some (later abstract) nodes which then
//! gets consumed individually and/or get refunded to the upper nodes.

use codec::{Decode, Encode};
use primitive_types::H256;
use sp_std::borrow::Cow;
use sp_std::prelude::*;

#[derive(Decode, Debug, Encode)]
enum ValueOrigin {
    External(H256),
    Local(H256),
}

impl Default for ValueOrigin {
    fn default() -> Self {
        ValueOrigin::External(H256::default())
    }
}

#[derive(Default, Decode, Debug, Encode)]
pub struct ValueNode {
    origin: ValueOrigin,
    refs: u32,
    inner: u64,
    consumed: bool,
}

pub enum ConsumeResult {
    NothingSpecial,
    RefundExternal(H256, u64),
}

#[derive(Debug)]
pub struct ValueView {
    prefix: Cow<'static, [u8]>,
    key: H256,
    node: ValueNode,
}

fn node_key(prefix: &[u8], node: &H256) -> Vec<u8> {
    [prefix, node.as_ref()].concat()
}

impl ValueView {
    pub fn get_or_create(
        prefix: impl Into<Cow<'static, [u8]>>,
        origin: H256,
        key: H256,
        value: u64,
    ) -> Self {
        let prefix: Cow<'static, [u8]> = prefix.into();

        let mut result = Self {
            prefix,
            key,
            node: ValueNode::default(),
        };

        match result.load_node(key) {
            Some(existing_node) => {
                result.node = existing_node;
            }
            None => {
                let node_key = node_key(result.prefix.as_ref(), &key);
                let root_node = ValueNode {
                    origin: ValueOrigin::External(origin),
                    refs: 0,
                    inner: value,
                    consumed: false,
                };
                sp_io::storage::set(&node_key, &root_node.encode());

                result.node = root_node;
            }
        }

        result
    }

    pub fn get(prefix: impl Into<Cow<'static, [u8]>>, key: H256) -> Option<Self> {
        let prefix: Cow<'static, [u8]> = prefix.into();

        let mut result = Self {
            prefix,
            key,
            node: ValueNode::default(),
        };

        result.node = match result.load_node(key) {
            Some(node) => node,
            None => return None,
        };

        Some(result)
    }

    fn new_from_node(&self, key: H256, node: ValueNode) -> Self {
        Self {
            prefix: self.prefix.clone(),
            key,
            node,
        }
    }

    fn check_consumed(self) -> ConsumeResult {
        match self.node.origin {
            ValueOrigin::Local(parent) => {
                if self.node.consumed && self.node.refs == 0 {
                    let mut parent_node = self
                        .load_node(parent)
                        .expect("Parent exist as link and should be loaded");

                    if parent_node.refs == 0 {
                        // should be an impossible situation
                        panic!(
                            "parent node does not contain ref for the node that was created from it"
                        );
                    }

                    parent_node.refs -= 1;
                    self.save_node(parent, &parent_node);
                    let result = self.new_from_node(parent, parent_node).check_consumed();
                    self.delete();

                    result
                } else {
                    ConsumeResult::NothingSpecial
                }
            }
            ValueOrigin::External(external) => {
                if self.node.refs == 0 && self.node.consumed {
                    let inner = self.node.inner;
                    self.delete();
                    ConsumeResult::RefundExternal(external, inner)
                } else {
                    ConsumeResult::NothingSpecial
                }
            }
        }
    }

    pub fn consume(mut self) -> ConsumeResult {
        match self.node.origin {
            ValueOrigin::Local(parent) => {
                let mut parent_node = self
                    .load_node(parent)
                    .expect("Parent exist as link and should be loaded");

                if parent_node.refs == 0 {
                    // should be an impossible situation
                    panic!(
                        "parent node does not contain ref for the node that was created from it"
                    );
                }

                parent_node.inner = parent_node.inner.saturating_add(self.node.inner);
                let mut delete_self = false;

                if self.node.refs == 0 {
                    delete_self = true;
                    parent_node.refs -= 1;
                } else {
                    self.node.consumed = true;
                    self.node.inner = 0;
                    self.save_node(self.key, &self.node);
                }

                self.save_node(parent, &parent_node);

                // now check if the parent node can be consumed as well
                let result = if parent_node.refs == 0 {
                    self.new_from_node(parent, parent_node).check_consumed()
                } else {
                    ConsumeResult::NothingSpecial
                };

                if delete_self {
                    self.delete()
                }

                result
            }
            ValueOrigin::External(external) => {
                self.node.consumed = true;
                self.save_node(self.key, &self.node);
                if self.node.refs == 0 {
                    let inner = self.node.inner;
                    self.delete();
                    ConsumeResult::RefundExternal(external, inner)
                } else {
                    ConsumeResult::NothingSpecial
                }
            }
        }
    }

    pub fn spend(&mut self, amount: u64) {
        if self.node.inner < amount {
            panic!("The fact that amount in current node is enough to spend some amount should be checked by caller!")
        }

        self.node.inner -= amount;

        self.save_node(self.key, &self.node);
    }

    pub fn split_off(&mut self, new_node_key: H256, new_value: u64) -> Self {
        if self.node.inner < new_value {
            panic!("The fact that amount in current node is enough to splitt off new value should be checked by caller!")
        }

        self.node.inner -= new_value;
        self.node.refs += 1;

        let new_node = ValueNode {
            origin: ValueOrigin::Local(self.key),
            inner: new_value,
            refs: 0,
            consumed: false,
        };

        self.save_node(new_node_key, &new_node);
        self.save_node(self.key, &self.node);

        Self {
            prefix: self.prefix.clone(),
            key: new_node_key,
            node: new_node,
        }
    }

    fn load_node(&self, id: H256) -> Option<ValueNode> {
        let node_key = node_key(self.prefix.as_ref(), &id);
        sp_io::storage::get(&node_key).map(|v| {
            ValueNode::decode(&mut &v[..]).expect("Value node should be encoded correctly")
        })
    }

    fn save_node(&self, id: H256, node: &ValueNode) {
        let node_key = node_key(self.prefix.as_ref(), &id);
        sp_io::storage::set(&node_key, &node.encode());
    }

    fn delete(self) {
        // TODO: delete key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_tree() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let new_root = H256::random();
            let origin = H256::random();

            let value_tree =
                ValueView::get_or_create(b"test::value_tree::".as_ref(), origin, new_root, 1000);

            let result = value_tree.consume();

            assert!(matches!(result, ConsumeResult::RefundExternal(_, 1000)));
        });
    }

    #[test]
    fn sub_nodes_tree() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let new_root = H256::random();
            let origin = H256::random();
            let split_1 = H256::random();
            let split_2 = H256::random();

            let mut value_tree =
                ValueView::get_or_create(b"test::value_tree::".as_ref(), origin, new_root, 1000);

            let split_off_1 = value_tree.split_off(split_1, 500);
            let split_off_2 = value_tree.split_off(split_2, 500);

            assert!(matches!(
                value_tree.consume(),
                ConsumeResult::NothingSpecial
            ));

            assert!(matches!(
                split_off_1.consume(),
                ConsumeResult::NothingSpecial
            ));

            assert!(matches!(
                split_off_2.consume(),
                ConsumeResult::RefundExternal(e, 1000) if e == origin,
            ));
        });
    }

    #[test]
    fn sub_nodes_tree_with_spends() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let new_root = H256::random();
            let origin = H256::random();

            let mut value_tree =
                ValueView::get_or_create(b"test::value_tree::".as_ref(), origin, new_root, 1000);

            let mut split_off_1 = value_tree.split_off(H256::random(), 500);
            let split_off_2 = value_tree.split_off(H256::random(), 500);

            split_off_1.spend(100);

            assert!(matches!(
                split_off_1.consume(),
                ConsumeResult::NothingSpecial
            ));
            assert!(matches!(
                split_off_2.consume(),
                ConsumeResult::NothingSpecial,
            ));

            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), new_root).expect("Should still exist").consume(),
                ConsumeResult::RefundExternal(e, 900) if e == origin,
            ));
        });
    }
}
