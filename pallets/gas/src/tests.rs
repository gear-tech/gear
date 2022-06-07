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
use frame_support::{assert_noop, assert_ok};
use primitive_types::H256;

use core::{
    iter::FromIterator,
    ops::{Deref, DerefMut, Index, Rem},
    slice::SliceIndex,
};
use std::collections::BTreeSet;

use proptest::prelude::*;
use proptest_derive::*;

type Gas = Pallet<Test>;

const MAX_ACTIONS: usize = 100;

// Substrate H256 primitive, which implements `Arbitrary`
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Arbitrary)]
struct OurH256(#[proptest(strategy = "any::<[u8; 32]>().prop_map(|v| H256::from_slice(&v))")] H256);

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

struct GasTreePropTester {
    origin: H256,
    nodes: Vec<H256>,
}

impl GasTreePropTester {
    fn new(root_balance: u64) -> Self {
        let origin = H256::random();
        let nodes = Self::default_nodes_with_root(MAX_ACTIONS);
        Gas::create(origin, nodes[0], root_balance).expect("new root creation failed");
        Self { origin, nodes }
    }

    fn build_tree(&mut self, actions: Vec<GasTreeAction>) {
        for action in actions {
            match action {
                GasTreeAction::SplitWithValue(parent_idx, amount) => {
                    if let Some(&parent) = self.nodes.ring_get(parent_idx) {
                        let child = H256::random();
                        // println!("split parent {:?} with value {:?}", parent, amount);

                        match Gas::split_with_value(parent, child, amount) {
                            Ok(_) => self.nodes.push(child),
                            Err(e) => {
                                // println!("{:?}", e)
                            }
                        };
                    }
                }
                GasTreeAction::Split(parent_idx) => {
                    if let Some(&parent) = self.nodes.ring_get(parent_idx) {
                        let child = H256::random();
                        // println!("split parent {:?}", parent);

                        match Gas::split(parent, child) {
                            Ok(_) => self.nodes.push(child),
                            Err(e) => {
                                // println!("{:?}", e)
                            }
                        };
                    }
                }
                GasTreeAction::Spend(from, amount) => {
                    if let Some(&from) = self.nodes.ring_get(from) {
                        // println!("spend from {:?}", from);
                        match Gas::spend(from, amount) {
                            Ok(_) => {}
                            Err(e) => {
                                // println!("{:?}", e)
                            }
                        };
                    }
                }
                GasTreeAction::Consume(id) => {
                    if let Some(&consuming) = self.nodes.ring_get(id) {
                        // println!("consume {:?}", consuming);
                        let len_before_consume = self.nodes.len();
                        match Gas::consume(consuming) {
                            Ok(v) => {
                                // println!("remove result! {:?}", v);
                                let keys_len = super::GasTree::<Test>::iter_keys().count();
                                if keys_len != len_before_consume {
                                    self.nodes =
                                        super::GasTree::<Test>::iter_keys().collect::<Vec<_>>();
                                }
                            }
                            Err(e) => {
                                // println!("{:?}", e)
                            }
                        }
                    }
                }
            }
            // println!("NODES {:?}", self.nodes);
            // println!("\n");
        }
    }

    fn default_nodes_with_root(cap: usize) -> Vec<H256> {
        let mut nodes = Vec::with_capacity(cap);
        nodes.push(H256::random());
        nodes
    }

    fn root(&self) -> H256 {
        self.nodes.first().copied().expect("root is always set")
    }

    fn nodes(&self) -> BTreeSet<H256> {
        BTreeSet::from_iter(super::GasTree::<Test>::iter_values().map(|v| v.id))
    }
}

proptest! {
    #[test]
    fn test_parents((max_tree_node_balance, actions) in any::<u64>().prop_flat_map(|max_balance| {
        (Just(max_balance), gas_action_strategy(max_balance))
    })) {
        new_test_ext().execute_with(|| {
            // Test whether all non external nodes have parents
            let mut t = GasTreePropTester::new(max_tree_node_balance);
            t.build_tree(actions);
            let existing_nodes = t.nodes();

            let gas_tree = super::GasTree::<Test>::iter_values().collect::<Vec<_>>();
            for node in gas_tree {
                if let Some(parent) = node.parent() {
                    assert!(existing_nodes.contains(&parent));
                }
            }
        })
    }

    #[test]
    fn test_ancestor_with_value((max_tree_node_balance, actions) in any::<u64>().prop_flat_map(|max_balance| {
        (Just(max_balance), gas_action_strategy(max_balance))
    })) {
        new_test_ext().execute_with(|| {
            // Test whether all non external nodes have parents
            let mut t = GasTreePropTester::new(max_tree_node_balance);
            t.build_tree(actions);
            let existing_nodes = t.nodes();

            let gas_tree = super::GasTree::<Test>::iter_values().collect::<Vec<_>>();
            for node in gas_tree {
                // node is a self ancestor as well
                let ancestor_with_value = node.node_with_value::<Test>().expect("can't fail");
                assert!(ancestor_with_value.inner_value().is_some())
            }
        })
    }
}

// test [sab] test gas tree invariants and all the branches
// 1/ If current non external exists, parent also exists
// 2/ Upstream node with a concrete value must exist for any node
// 3/ Если в consume функции нельзя удалить чайлда, то тем более нельзя удалить и parent-а
// 4/ нода может быть помечена как консьюмд, но не быть удалена (посему иметь рефсы). То есть, у любой
// у любой существующей ноды между вызлвами есть рефсы.
// проверять на consumed при удалении (а не только на рефсы) правильно, потому что это есть ситуации, когда родитель не консьюмд, а потомк консьюмд. Например
// при вызове wait, до которого было отправлено сообщение
// 9/ consumed можно стать только в consume, а не чек-консьюмд
// 10/ Из 9 следует, что при вызове чек-консьюмд узел будет иметь газ только в том случае, если у него на момент вызова consume (на нем)
// был не сконсьюмленный ан-спек чайлд (и это в том случае, если мы провалились в ветку с удалением узла в чек консьюмд)
// 11/ Предок удаляется только при 2-ух условиях сразу: 1. над ним вызвали консьюм, 2. над всеми его чайладми его вызвали и они
// были удалены.
// 12/ дерево не создастся без корня external узла
// 13/ если у корня баланс мелкий, то любые сплиты с большим балансом будут падать

// Проверяй чтобы нью кей и кей в сплитах не были одинаковы.

// отдельные тесты
// split над сonsumed
// сделай тест, специальный про состояние дерева, когда постоянно делаешь spend/split_value с количеством большим, чем возможно

#[test]
fn simple_value_tree() {
    new_test_ext().execute_with(|| {
        let new_root = H256::random();

        {
            let pos = Gas::create(ALICE.into_origin(), new_root, 1000).unwrap();

            assert_eq!(pos.peek(), 1000);
        }
        // Positive imbalance dropped - the total issuance should have been upped to 1000
        assert_eq!(Gas::total_supply(), 1000);

        {
            let (_neg, owner) = Gas::consume(new_root).unwrap().unwrap();

            assert_eq!(owner, ALICE.into_origin());
        }
        // Total supply back to original value
        assert_eq!(Gas::total_supply(), 0);
    });
}

#[test]
fn sub_nodes_tree() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = H256::random();
        let origin = H256::random();
        let split_1 = H256::random();
        let split_2 = H256::random();

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();
        assert_eq!(pos_imb.peek(), 1000);

        assert_ok!(Gas::split_with_value(new_root, split_1, 500));
        assert_ok!(Gas::split_with_value(new_root, split_2, 500));
        // No new value created - total supply not affected
        assert_eq!(pos_imb.peek(), 1000);

        // We must drop the imbalance to reflect changes in total supply
        drop(pos_imb);
        assert_eq!(Gas::total_supply(), 1000);

        assert!(matches!(Gas::consume(new_root).unwrap(), None));
        assert!(matches!(Gas::consume(split_1).unwrap(), None));

        let consume_result = Gas::consume(split_2).unwrap();
        assert!(consume_result.is_some());
        assert_eq!(consume_result.unwrap().0.peek(), 1000);
        // Negative imbalance moved and dropped above - total supply decreased
        assert_eq!(Gas::total_supply(), 0);
    });
}

#[test]
fn value_tree_known_errors() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = H256::random();
        let origin = H256::random();
        let split_1 = H256::random();
        let split_2 = H256::random();

        {
            let pos_imb = Gas::create(origin, new_root, 1000).unwrap();
            assert_eq!(pos_imb.peek(), 1000);

            // Attempt to re-create an existing node
            assert_noop!(
                Gas::create(origin, new_root, 1000),
                Error::<Test>::GasTreeAlreadyExists
            );

            // Try to split on non-existent node
            assert_noop!(
                Gas::split_with_value(split_2, split_1, 500),
                Error::<Test>::NodeNotFound
            );

            // Try to split with excessive balance
            assert_noop!(
                Gas::split_with_value(new_root, split_1, 5000),
                Error::<Test>::InsufficientBalance
            );

            // Total supply not affected so far - imbalance is not yet dropped
            assert_eq!(pos_imb.peek(), 1000);
            assert_eq!(Gas::total_supply(), 0);

            // Consume node
            assert_ok!(Gas::consume(new_root));
            // Negative imbalance dropped immediately - total supply decreased (in saturating way)
            // In practice it means the total supply is still 0
            assert_eq!(Gas::total_supply(), 0);
        }
        // Now initial positive imbalance has been dropped
        // which should have affected the total supply
        assert_eq!(Gas::total_supply(), 1000);

        // TODO: dropping imbalances in wrong order can lead to incorrect total supply value
    });
}

#[test]
fn sub_nodes_tree_with_spends() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = H256::random();
        let origin = H256::random();
        let split_1 = H256::random();
        let split_2 = H256::random();

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split_with_value(new_root, split_1, 500));
        assert_ok!(Gas::split_with_value(new_root, split_2, 500));

        let offset1 = pos_imb
            .offset(Gas::spend(split_1, 100).unwrap())
            .same()
            .unwrap();
        assert_eq!(offset1.peek(), 900);

        assert!(matches!(Gas::consume(split_1).unwrap(), None));
        assert!(matches!(Gas::consume(split_2).unwrap(), None));

        let offset2 = offset1
            .offset(Gas::consume(new_root).unwrap().unwrap().0)
            .same()
            .unwrap();
        assert_eq!(offset2.peek(), 0);

        assert_ok!(offset2.drop_zero());
        assert_eq!(Gas::total_supply(), 0);
    });
}

#[test]
fn all_keys_are_cleared() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let root = H256::random();
        let origin = H256::random();
        let sub_keys = (0..5).map(|_| H256::random()).collect::<Vec<_>>();

        Gas::create(origin, root, 2000).unwrap();
        for key in sub_keys.iter() {
            Gas::split_with_value(root, *key, 100).unwrap();
        }

        assert_ok!(Gas::consume(root));
        for key in sub_keys.iter() {
            // here we have not yet consumed everything
            assert!(GasTree::<Test>::contains_key(*key));

            assert_ok!(Gas::consume(*key));
        }

        // here we consumed everything
        let key_count = GasTree::<Test>::iter_keys().fold(0, |k, _| k + 1);
        assert_eq!(key_count, 0);
    });
}

#[test]
fn split_with_no_value() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = H256::random();
        let origin = H256::random();
        let split_1 = H256::random();
        let split_2 = H256::random();
        let split_1_2 = H256::random();

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split(new_root, split_1));
        assert_ok!(Gas::split(new_root, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_2, 500));

        let offset1 = pos_imb
            .offset(Gas::spend(split_1_2, 100).unwrap())
            .same()
            .unwrap();
        assert_eq!(offset1.peek(), 900);
        assert_eq!(Gas::spend(split_1, 200).unwrap().peek(), 200);

        assert!(matches!(Gas::consume(split_1).unwrap(), None));
        assert!(matches!(Gas::consume(split_2).unwrap(), None));
        assert!(matches!(Gas::consume(split_1_2).unwrap(), None));

        let final_imb = Gas::consume(new_root).unwrap().unwrap().0;
        assert_eq!(final_imb.peek(), 700);

        assert_eq!(Gas::total_supply(), 0);
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

        assert_ok!(Gas::create(origin, root, 2000));

        assert_ok!(Gas::split_with_value(root, m1, 1500));
        assert_ok!(Gas::split_with_value(m1, m2, 1000));
        assert_ok!(Gas::split_with_value(m2, m3, 500));
        assert_ok!(Gas::split_with_value(m3, m4, 250));

        assert_ok!(Gas::spend(root, 50));
        assert_ok!(Gas::spend(m1, 50));
        assert_ok!(Gas::spend(m2, 50));
        assert_ok!(Gas::spend(m3, 50));
        assert_ok!(Gas::spend(m4, 50));

        assert!(matches!(Gas::consume(m1).unwrap(), None));
        assert!(matches!(Gas::consume(m2).unwrap(), None));
        assert!(matches!(Gas::consume(root).unwrap(), None));
        assert!(matches!(Gas::consume(m4).unwrap(), None));

        let (neg_imb, payee) = Gas::consume(m3).unwrap().unwrap();

        // 2000 initial, 5*50 spent
        assert_eq!(neg_imb.peek(), 1750);
        assert_eq!(payee, origin);
    });
}

#[test]
fn limit_vs_origin() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = H256::random();
        let root_node = H256::random();
        let split_1 = H256::random();
        let split_2 = H256::random();
        let split_1_1 = H256::random();
        let split_1_2 = H256::random();
        let split_1_1_1 = H256::random();

        assert_ok!(Gas::create(origin, root_node, 1000));

        assert_ok!(Gas::split(root_node, split_1));
        assert_ok!(Gas::split(root_node, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_1, 600));
        assert_ok!(Gas::split(split_1, split_1_2));
        assert_ok!(Gas::split(split_1_1, split_1_1_1));

        // Original 1000 less 600 that were `split_with_value`
        assert_eq!(Gas::get_limit(root_node).unwrap(), Some(400));

        // Parent's 400
        assert_eq!(Gas::get_limit(split_1).unwrap(), Some(400));

        // Parent's 400
        assert_eq!(Gas::get_limit(split_2).unwrap(), Some(400));

        // Propriatery 600
        assert_eq!(Gas::get_limit(split_1_1).unwrap(), Some(600));

        // Grand-parent's 400
        assert_eq!(Gas::get_limit(split_1_2).unwrap(), Some(400));

        // Parent's 600
        assert_eq!(Gas::get_limit(split_1_1_1).unwrap(), Some(600));

        // All nodes origin is `origin`
        assert_eq!(Gas::get_origin(root_node).unwrap(), Some(origin));
        assert_eq!(Gas::get_origin(split_1).unwrap(), Some(origin));
        assert_eq!(Gas::get_origin(split_2).unwrap(), Some(origin));
        assert_eq!(Gas::get_origin(split_1_1).unwrap(), Some(origin));
        assert_eq!(Gas::get_origin(split_1_2).unwrap(), Some(origin));
        assert_eq!(Gas::get_origin(split_1_1_1).unwrap(), Some(origin));
    });
}

#[test]
fn subtree_gas_limit_remains_intact() {
    // Consider the following gas tree configuration:
    //
    //                          root
    //                      (external: 200)
    //                            |
    //                            |
    //                          node_1
    //                     (specified: 300)
    //                      /           \
    //                     /             \
    //                 node_2           node_3
    //            (specified: 250)    (unspecified)
    //             /           \
    //            /             \
    //        node_4           node_5
    //    (unspecified)    (specified: 250)
    //
    // Total value locked in the tree is 1000.
    // node_3 defines the gas limit for its child nodes with unspecified limit (node_4)
    // node_1 defines the gas limit for its right subtree - node_3
    // Regardless which nodes are consumed first, the gas limit of each "unspecified" node
    // must remain exactly as it was initially set: not more, not less.
    //
    // In the test scenario node_1 is consumed first, and then node_2 is consumed.
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = H256::random();
        let root = H256::random();
        let node_1 = H256::random();
        let node_2 = H256::random();
        let node_3 = H256::random();
        let node_4 = H256::random();
        let node_5 = H256::random();

        // Prepare the initial configuration
        assert_ok!(Gas::create(origin, root, 1000));
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));
        assert_ok!(Gas::split(node_2, node_4));
        assert_ok!(Gas::split_with_value(node_2, node_5, 250));

        // Check gas limits in the beginning
        assert_eq!(Gas::get_limit(root).unwrap(), Some(200));
        assert_eq!(Gas::get_limit(node_1).unwrap(), Some(300));
        assert_eq!(Gas::get_limit(node_2).unwrap(), Some(250));
        assert_eq!(Gas::get_limit(node_3).unwrap(), Some(300)); // defined by parent
        assert_eq!(Gas::get_limit(node_4).unwrap(), Some(250)); // defined by parent
        assert_eq!(Gas::get_limit(node_5).unwrap(), Some(250));

        // Consume node_1
        assert!(matches!(Gas::consume(node_1).unwrap(), None));
        // Expect gas limit of the node_3 to remain unchanged
        assert_eq!(Gas::get_limit(node_3).unwrap(), Some(300));

        // Consume node_2
        assert!(matches!(Gas::consume(node_2).unwrap(), None));
        // Expect gas limit of the node_4 to remain unchanged
        assert_eq!(Gas::get_limit(node_4).unwrap(), Some(250));
    });
}
