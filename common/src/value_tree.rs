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

#[allow(clippy::derivable_impls)]
// this cannot be derived, despite clippy is saying this!!
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

#[derive(Debug, PartialEq)]
pub enum ConsumeResult {
    None,
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

                    assert!(
                        !(parent_node.refs == 0),
                        "parent node does not contain ref for the node that was created from it"
                    );

                    parent_node.refs -= 1;
                    parent_node.inner = parent_node.inner.saturating_add(self.node.inner);

                    self.save_node(parent, &parent_node);
                    let result = self.new_from_node(parent, parent_node).check_consumed();
                    self.delete();

                    result
                } else {
                    ConsumeResult::None
                }
            }
            ValueOrigin::External(external) => {
                if self.node.refs == 0 && self.node.consumed {
                    let inner = self.node.inner;
                    self.delete();
                    ConsumeResult::RefundExternal(external, inner)
                } else {
                    ConsumeResult::None
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

                assert!(
                    !(parent_node.refs == 0),
                    "parent node does not contain ref for the node that was created from it"
                );

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
                    ConsumeResult::None
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
                    ConsumeResult::None
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

    pub fn origin(&self) -> H256 {
        match self.node.origin {
            ValueOrigin::External(external_origin) => external_origin,
            ValueOrigin::Local(parent) => ValueView::get(self.prefix.clone(), parent)
                .expect("Parent should exist")
                .origin(),
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
        let node_key = node_key(self.prefix.as_ref(), &self.key);
        sp_io::storage::clear(node_key.as_ref());
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

            assert!(matches!(value_tree.consume(), ConsumeResult::None));

            assert!(matches!(split_off_1.consume(), ConsumeResult::None));

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
                ConsumeResult::None
            ));
            assert!(matches!(
                split_off_2.consume(),
                ConsumeResult::None,
            ));

            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), new_root).expect("Should still exist").consume(),
                ConsumeResult::RefundExternal(e, 900) if e == origin,
            ));
        });
    }

    #[test]
    fn all_keys_are_cleared() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let root = H256::random();
            let origin = H256::random();
            let sub_keys = (0..5).map(|_| H256::random()).collect::<Vec<_>>();

            let mut next =
                ValueView::get_or_create(b"test::value_tree::".as_ref(), origin, root, 2000);
            for key in sub_keys.iter() {
                next = next.split_off(*key, 100);
            }

            ValueView::get(b"test::value_tree::".as_ref(), root)
                .expect("Should still exist")
                .consume();
            for key in sub_keys.iter() {
                // here we are not yet consumed everything
                let any_key_under_prefix = sp_io::storage::next_key(b"test::value_tree::")
                    .filter(|key| key.starts_with(b"test::value_tree::"));
                assert!(any_key_under_prefix.is_some());

                ValueView::get(b"test::value_tree::".as_ref(), *key)
                    .expect("Should still exist")
                    .consume();
            }

            // here we consumed everything
            let any_key_under_prefix = sp_io::storage::next_key(b"test::value_tree::")
                .filter(|key| key.starts_with(b"test::value_tree::"));
            assert!(any_key_under_prefix.is_none());
        });
    }

    #[test]
    fn long_chain() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let root = H256::random();
            let m1 = H256::random();
            let m2 = H256::random();
            let m3 = H256::random();
            let m4 = H256::random();
            let origin = H256::random();

            let mut value_tree =
                ValueView::get_or_create(b"test::value_tree::".as_ref(), origin, root, 2000);

            let mut split_off_1 = value_tree.split_off(m1, 1500);
            let mut split_off_2 = split_off_1.split_off(m2, 1000);
            let mut split_off_3 = split_off_2.split_off(m3, 500);
            let _split_off_4 = split_off_3.split_off(m4, 250);

            ValueView::get(b"test::value_tree::".as_ref(), root)
                .expect("Should still exist")
                .spend(50);
            ValueView::get(b"test::value_tree::".as_ref(), m1)
                .expect("Should still exist")
                .spend(50);
            ValueView::get(b"test::value_tree::".as_ref(), m2)
                .expect("Should still exist")
                .spend(50);
            ValueView::get(b"test::value_tree::".as_ref(), m3)
                .expect("Should still exist")
                .spend(50);
            ValueView::get(b"test::value_tree::".as_ref(), m4)
                .expect("Should still exist")
                .spend(50);

            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), m1)
                    .expect("Should still exist")
                    .consume(),
                ConsumeResult::None
            ));
            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), m2)
                    .expect("Should still exist")
                    .consume(),
                ConsumeResult::None
            ));
            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), root)
                    .expect("Should still exist")
                    .consume(),
                ConsumeResult::None
            ));
            assert!(matches!(
                ValueView::get(b"test::value_tree::".as_ref(), m4)
                    .expect("Should still exist")
                    .consume(),
                ConsumeResult::None
            ));

            // 2000 initial, 5*50 spent
            assert_eq!(
                ValueView::get(b"test::value_tree::".as_ref(), m3)
                    .expect("Should still exist")
                    .consume(),
                ConsumeResult::RefundExternal(origin, 1750),
            );
        });
    }
}
