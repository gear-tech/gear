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
use common::{GasTree as _, Origin, gas_provider::{PositiveImbalance, NegativeImbalance}};
use frame_support::{assert_noop, assert_ok, traits::Imbalance};
use gear_core::ids::MessageId;
use primitive_types::H256;

type Gas = <Pallet<Test> as common::GasProvider>::GasTree;
type GasTree = GasNodes<Test>;

#[test]
fn simple_value_tree() {
    new_test_ext().execute_with(|| {
        let new_root = MessageId::from_origin(H256::random());

        {
            let pos = Gas::create(ALICE, new_root, 1000).unwrap();

            assert_eq!(pos.peek(), 1000);
            assert_eq!(Gas::total_supply(), 0);
        }
        // Positive imbalance dropped - the total issuance should have been upped to 1000
        assert_eq!(Gas::total_supply(), 1000);

        {
            let (_neg, owner) = Gas::consume(new_root).unwrap().unwrap();

            assert_eq!(owner, ALICE);
            assert_eq!(Gas::total_supply(), 1000);
        }
        // Total supply back to original value
        assert_eq!(Gas::total_supply(), 0);
    });
}

#[test]
fn test_consume_procedure_with_subnodes() {
    new_test_ext().execute_with(|| {
        let origin = H256::random();
        let root = MessageId::from_origin(H256::random());
        let node_1 = MessageId::from_origin(H256::random());
        let node_2 = MessageId::from_origin(H256::random());
        let node_3 = MessageId::from_origin(H256::random());
        let node_4 = MessageId::from_origin(H256::random());

        let pos_imb = Gas::create(origin, root, 300).unwrap();
        assert_eq!(pos_imb.peek(), 300);
        // Chain of nodes, that form more likely a path rather then a tree
        assert_ok!(Gas::split_with_value(root, node_1, 200));
        assert_ok!(Gas::split_with_value(root, node_2, 100));
        assert_ok!(Gas::split_with_value(node_1, node_3, 100));
        assert_ok!(Gas::split(node_3, node_4));

        assert_eq!(Gas::total_supply(), 0);

        // We must drop the imbalance to reflect changes in total supply
        drop(pos_imb);
        assert_eq!(Gas::total_supply(), 300);

        // Consume root
        let consume_root = Gas::consume(root);
        assert!(consume_root.is_ok());
        assert_eq!(consume_root.unwrap().unwrap().0.peek(), 0);
        // total supply mustn't be affected, because root sponsored all it's balance
        assert_eq!(Gas::total_supply(), 300);
        // Consumed still exists, but has no balance.
        assert_eq!(Gas::get_limit(root).unwrap(), Some((0, root)));

        // Consume node_1
        let consume_node_1 = Gas::consume(node_1);
        assert!(consume_node_1.is_ok());
        // Consumed node without unspec refs returns value,
        assert_eq!(consume_node_1.unwrap().unwrap().0.peek(), 100);
        // So it has no balance, but exists due to having children
        assert_eq!(Gas::get_limit(node_1).unwrap(), Some((0, root)));
        // total supply is affected
        assert_eq!(Gas::total_supply(), 200);
        // Check value wasn't moved up to the root
        assert_eq!(Gas::get_limit(root).unwrap(), Some(0));

        // Consume node_2 independently
        let consume_node_2 = Gas::consume(node_2);
        assert!(consume_node_2.is_ok());
        // The node is not a patron, so value should be returned
        assert_eq!(consume_node_2.unwrap().unwrap().0.peek(), 100);
        // It has no children, so should be removed
        assert_eq!(Gas::get_limit(node_2).unwrap(), None);
        // Total supply is affected
        assert_eq!(Gas::total_supply(), 100);

        // Consume node_3
        assert_eq!(Gas::consume(node_3).unwrap(), None);
        // Consumed node with unspec refs doesn't moves value up
        assert_eq!(Gas::get_limit(node_3).unwrap(), Some(100));
        // Check that spending from unspec `node_4` actually decreases balance from the ancestor with value - `node_3`.
        assert_eq!(Gas::spend(node_4, 100).unwrap().peek(), 100);
        assert_eq!(Gas::get_limit(node_3).unwrap(), Some(0));
        // total supply is affected after spending all of the blockage `node_3`
        assert_eq!(Gas::total_supply(), 0);
        // Still exists, although is consumed and has a zero balance. The only way to remove it is to remove children.
        assert_noop!(Gas::consume(node_3), Error::<Test>::NodeWasConsumed,);

        // Impossible to consume non-existing node.
        assert_noop!(Gas::consume(H256::random()), Error::<Test>::NodeNotFound,);

        // Before consuming blockage `node_4`
        assert!(Gas::get_node(root).is_some());
        assert!(Gas::get_node(node_1).is_some());
        assert!(Gas::get_node(node_3).is_some());

        let consume_node_4 = Gas::consume(node_4);
        assert!(consume_node_4.is_ok());
        assert_eq!(consume_node_4.unwrap().unwrap().0.peek(), 0);

        // After consuming blockage `node_3`
        assert!(GasTree::iter_keys().next().is_none());
    })
}


#[test]
fn can_cut_nodes() {
    new_test_ext().execute_with(|| {
        let (root, specified, unspecified, cut_a, cut_b, cut_c) = (
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
        );
        let (total_supply, specified_value, cut_a_value, cut_b_value, cut_c_value) =
            (1000, 500, 300, 200, 100);

        // create nodes
        {
            assert_ok!(Gas::create(ALICE, root, total_supply));
            assert_ok!(Gas::cut(root, cut_a, cut_a_value));
            assert_ok!(Gas::split_with_value(root, specified, specified_value));
            assert_ok!(Gas::cut(specified, cut_b, cut_b_value));
            assert_ok!(Gas::split(root, unspecified));
            assert_ok!(Gas::cut(unspecified, cut_c, cut_c_value));
        }

        assert_eq!(Gas::total_supply(), total_supply);

        assert_eq!(
            Gas::get_limit(root).map(|gas_limit| gas_limit.map(|(g, _)| g)),
            Ok(Some(
                total_supply - specified_value - cut_a_value - cut_c_value
            ))
        );

        assert_eq!(
            Gas::get_limit(specified).map(|gas_limit| gas_limit.map(|(g, _)| g)),
            Ok(Some(specified_value - cut_b_value))
        );

        assert_eq!(
            Gas::get_limit(cut_a).map(|gas_limit| gas_limit.map(|(g, _)| g)),
            Ok(Some(cut_a_value))
        );
        assert_eq!(
            Gas::get_limit(cut_b).map(|gas_limit| gas_limit.map(|(g, _)| g)),
            Ok(Some(cut_b_value))
        );
        assert_eq!(
            Gas::get_limit(cut_c).map(|gas_limit| gas_limit.map(|(g, _)| g)),
            Ok(Some(cut_c_value))
        );
    })
}

#[test]
fn value_tree_with_all_kinds_of_nodes() {
    new_test_ext().execute_with(|| {
        let total_supply = 1000;
        let cut_value = 300;
        let specified_value = total_supply - cut_value;
        let (root, cut, specified, unspecfied) = (
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
            MessageId::from_origin(H256::random()),
        );

        // create nodes
        {
            assert_ok!(Gas::create(ALICE, root, total_supply));
            assert_ok!(Gas::cut(root, cut, cut_value));
            assert_ok!(Gas::split_with_value(root, specified, specified_value));
            assert_ok!(Gas::split(root, unspecfied));
        }

        assert_eq!(Gas::total_supply(), total_supply);

        // consume nodes
        {
            assert_eq!(Gas::consume(unspecfied), Ok(None));
            assert!(matches!(Gas::consume(specified), Ok(Some(_))));
            assert_eq!(Gas::total_supply(), total_supply - specified_value);

            assert_eq!(
                Gas::consume(root),
                Ok(Some((
                    common::gas_provider::NegativeImbalance::new(0),
                    ALICE
                )))
            );
            assert_eq!(
                Gas::consume(cut),
                Ok(Some((
                    common::gas_provider::NegativeImbalance::new(cut_value),
                    ALICE
                )))
            );
        }

        assert_eq!(Gas::total_supply(), 0);
    })
}

#[test]
fn splits_fail() {
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
    //
    new_test_ext().execute_with(|| {
        let origin = ALICE;
        let root = MessageId::from_origin(H256::random());
        let node_1 = MessageId::from_origin(H256::random());
        let node_2 = MessageId::from_origin(H256::random());
        let node_3 = MessageId::from_origin(H256::random());

        // Prepare the initial configuration
        assert_ok!(Gas::create(origin, root, 1000));
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));

        assert_eq!(Gas::consume(node_1).unwrap(), None);
        // Can actually split consumed
        assert_ok!(Gas::split(node_1, MessageId::from_origin(H256::random())));

        // Can't split with existing id.
        assert_noop!(Gas::split(root, node_2), Error::<Test>::NodeAlreadyExists,);
        // Special case is when provided 2 existing equal ids
        assert_noop!(Gas::split(node_2, node_2), Error::<Test>::NodeAlreadyExists,);
        // Not equal ids can be caught as well
        let node_4 = MessageId::from_origin(H256::random());
        assert_noop!(Gas::split(node_4, node_4), Error::<Test>::NodeNotFound,);
    })
}

#[test]
fn value_tree_known_errors() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = MessageId::from_origin(H256::random());
        let origin = ALICE;
        let split_1 = MessageId::from_origin(H256::random());
        let split_2 = MessageId::from_origin(H256::random());
        let cut = MessageId::from_origin(H256::random());
        let cut_1 = MessageId::from_origin(H256::random());

        {
            let pos_imb = Gas::create(origin, new_root, 1000).unwrap();
            assert_eq!(pos_imb.peek(), 1000);

            // Cut a reserved node
            assert_ok!(Gas::cut(new_root, cut, 100));

            // Attempt to re-create an existing node
            assert_noop!(
                Gas::create(origin, new_root, 1000),
                Error::<Test>::NodeAlreadyExists
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

            assert_ok!(Gas::split(new_root, split_1));
            assert_ok!(Gas::split(new_root, split_2));

            assert_ok!(Gas::spend(split_1, 100));
            assert_ok!(Gas::spend(split_2, 100));

            assert_eq!(Gas::get_limit(new_root), Ok(Some(800)));
            // Try to split the reserved node
            assert_noop!(Gas::split(cut, split_1), Error::<Test>::Forbidden);

            // Try to split the reserved node with value
            assert_noop!(
                Gas::split_with_value(cut, split_1, 50),
                Error::<Test>::Forbidden
            );

            // Try to cut the reserved node
            assert_noop!(Gas::cut(cut, cut_1, 50), Error::<Test>::Forbidden);

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
        let new_root = MessageId::from_origin(H256::random());
        let origin = ALICE;
        let split_1 = MessageId::from_origin(H256::random());
        let split_2 = MessageId::from_origin(H256::random());

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split_with_value(new_root, split_1, 500));
        assert_ok!(Gas::split_with_value(new_root, split_2, 500));

        let offset1 = pos_imb
            .offset(Gas::spend(split_1, 100).unwrap())
            .same()
            .unwrap();
        assert_eq!(offset1.peek(), 900);

        // Because root is not consumed, it is considered as a patron
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
        let root = MessageId::from_origin(H256::random());
        let origin = ALICE;
        let sub_keys = (0..5)
            .map(|_| MessageId::from_origin(H256::random()))
            .collect::<Vec<_>>();

        Gas::create(origin, root, 2000).unwrap();
        for key in sub_keys.iter() {
            Gas::split_with_value(root, *key, 100).unwrap();
        }

        assert_ok!(Gas::consume(root));
        for key in sub_keys.iter() {
            // here we have not yet consumed everything
            assert!(GasTree::contains_key(*key));

            // There are no patron nodes in the tree after root was consumed
            assert!(matches!(Gas::consume(*key).unwrap(), Some(_)));
        }

        // here we consumed everything
        let key_count = GasTree::iter_keys().fold(0, |k, _| k + 1);
        assert_eq!(key_count, 0);
    });
}

#[test]
fn split_with_no_value() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let new_root = MessageId::from_origin(H256::random());
        let origin = ALICE;
        let split_1 = MessageId::from_origin(H256::random());
        let split_2 = MessageId::from_origin(H256::random());
        let split_1_2 = MessageId::from_origin(H256::random());

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
        // gas-less nodes are always leaves, so easily removed
        assert!(Gas::get_node(split_1).is_none());
        assert!(Gas::get_node(split_2).is_none());

        // Returns None, because root is not consumed, so considered as a patron
        assert!(matches!(Gas::consume(split_1_2).unwrap(), None));

        let final_imb = Gas::consume(new_root).unwrap().unwrap().0;
        assert_eq!(final_imb.peek(), 700);

        assert_eq!(Gas::total_supply(), 0);
    });
}

#[test]
fn long_chain() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let root = MessageId::from_origin(H256::random());
        let m1 = MessageId::from_origin(H256::random());
        let m2 = MessageId::from_origin(H256::random());
        let m3 = MessageId::from_origin(H256::random());
        let m4 = MessageId::from_origin(H256::random());
        let origin = ALICE;

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

        let root_expected_limit = 450;
        let m1_expected_limit = 450;
        let m2_expected_limit = 450;
        assert_eq!(Gas::get_limit(root).unwrap(), Some(root_expected_limit));
        assert_eq!(Gas::get_limit(m1).unwrap(), Some(m1_expected_limit));
        assert_eq!(Gas::get_limit(m2).unwrap(), Some(m2_expected_limit));
        assert_eq!(Gas::get_limit(m3).unwrap(), Some(200));
        assert_eq!(Gas::get_limit(m4).unwrap(), Some(200));

        // Send their value to the root, which is not consumed. therefore considered as a patron
        assert!(matches!(Gas::consume(m1).unwrap(), None));
        assert!(matches!(Gas::consume(m2).unwrap(), None));
        // Doesn't have any unspec refs. so not a patron
        assert!(matches!(Gas::consume(root).unwrap(), Some(_)));
        // Has a patron parent m3
        assert!(matches!(Gas::consume(m4).unwrap(), None));

        let (neg_imb, payee) = Gas::consume(m3).unwrap().unwrap();

        // 2000 initial, 5*50 spent
        assert_eq!(
            neg_imb.peek(),
            1750 - root_expected_limit - m1_expected_limit - m2_expected_limit
        );
        assert_eq!(payee, origin);
    });
}

#[test]
fn limit_vs_origin() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = BOB;
        let root_node = MessageId::from_origin(H256::random());
        let cut = MessageId::from_origin(H256::random());
        let split_1 = MessageId::from_origin(H256::random());
        let split_2 = MessageId::from_origin(H256::random());
        let split_1_1 = MessageId::from_origin(H256::random());
        let split_1_2 = MessageId::from_origin(H256::random());
        let split_1_1_1 = MessageId::from_origin(H256::random());

        assert_ok!(Gas::create(origin, root_node, 1100));

        assert_ok!(Gas::cut(root_node, cut, 300));
        assert_ok!(Gas::split(root_node, split_1));
        assert_ok!(Gas::split(root_node, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_1, 600));
        assert_ok!(Gas::split(split_1, split_1_2));
        assert_ok!(Gas::split(split_1_1, split_1_1_1));

        // Original 1100 less 200 that were `cut` and `split_with_value`
        assert_eq!(Gas::get_limit(root_node).unwrap(), Some((200, root_node)));

        // 300 cut from the root node
        assert_eq!(Gas::get_limit(cut).unwrap(), Some((300, cut)));

        // Parent's 200
        assert_eq!(Gas::get_limit(split_1).unwrap(), Some((200, root_node)));

        // Parent's 200
        assert_eq!(Gas::get_limit(split_2).unwrap(), Some((200, root_node)));

        // Propriatery 600
        assert_eq!(Gas::get_limit(split_1_1).unwrap(), Some((600, split_1_1)));

        // Grand-parent's 200
        assert_eq!(Gas::get_limit(split_1_2).unwrap(), Some((200, root_node)));

        // Parent's 600
        assert_eq!(Gas::get_limit(split_1_1_1).unwrap(), Some((600, split_1_1)));

        // All nodes origin is `origin`
        assert_eq!(Gas::get_external(root_node).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(cut).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(split_1).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(split_2).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(split_1_1).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(split_1_2).unwrap(), Some(origin));
        assert_eq!(Gas::get_external(split_1_1_1).unwrap(), Some(origin));
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
    // Also an ability to spend value by "unspec" child from "spec" parent will be tested.
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = BOB;
        let root = MessageId::from_origin(H256::random());
        let node_1 = MessageId::from_origin(H256::random());
        let node_2 = MessageId::from_origin(H256::random());
        let node_3 = MessageId::from_origin(H256::random());
        let node_4 = MessageId::from_origin(H256::random());
        let node_5 = MessageId::from_origin(H256::random());

        // Prepare the initial configuration
        assert_ok!(Gas::create(origin, root, 1000));
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));
        assert_ok!(Gas::split(node_2, node_4));
        assert_ok!(Gas::split_with_value(node_2, node_5, 250));

        // Check gas limits in the beginning
        assert_eq!(Gas::get_limit(root).unwrap(), Some((200, root)));
        assert_eq!(Gas::get_limit(node_1).unwrap(), Some((300, node_1)));
        assert_eq!(Gas::get_limit(node_2).unwrap(), Some((250, node_2)));
        // defined by parent
        assert_eq!(Gas::get_limit(node_3).unwrap(), Some((300, node_1)));
        // defined by parent
        assert_eq!(Gas::get_limit(node_4).unwrap(), Some((250, node_2)));
        assert_eq!(Gas::get_limit(node_5).unwrap(), Some((250, node_5)));

        // Consume node_1
        assert!(matches!(Gas::consume(node_1).unwrap(), None));
        // Expect gas limit of the node_3 to remain unchanged
        assert_eq!(Gas::get_limit(node_3).unwrap().map(|(g, _)| g), Some(300));

        // Consume node_2
        assert!(matches!(Gas::consume(node_2).unwrap(), None));
        // Marked as consumed
        assert!(GasTree::get(node_2).map(|node| node.consumed).unwrap());
        // Expect gas limit of the node_4 to remain unchanged
        assert_eq!(Gas::get_limit(node_4).unwrap().map(|(g, _)| g), Some(250));

        // Consume node 5
        assert!(matches!(Gas::consume(node_5).unwrap(), None));
        // node_5 was removed
        assert_eq!(Gas::get_limit(node_5).unwrap(), None);
        // Expect gas limit from node_5 sent to upstream node with a value (node_2, which is consumed)
        assert_eq!(Gas::get_limit(node_2).unwrap().map(|(g, _)| g), Some(500));

        // Spend from unspecified node_4, which actually spends gas from node_2 (ancestor with value)
        assert_ok!(Gas::spend(node_4, 200));
        // Expect gas limit of consumed node_2 to decrease by 200 (thus checking we can spend from consumed node)
        assert_eq!(Gas::get_limit(node_2).unwrap().map(|(g, _)| g), Some(300));
        // Or explicitly spend from consumed node_2 by calling "spend"
        assert_ok!(Gas::spend(node_2, 200));
        assert_eq!(Gas::get_limit(node_2).unwrap().map(|(g, _)| g), Some(100));
    });
}

#[test]
fn gas_free_after_consumed() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = BOB;
        let root_msg_id = MessageId::from_origin(H256::random());

        assert_ok!(Gas::create(origin, root_msg_id, 1000));
        assert_ok!(Gas::spend(root_msg_id, 300));

        let (v, _) = Gas::consume(root_msg_id).unwrap().unwrap();
        assert_eq!(v.peek(), 700);
        assert_eq!(Gas::get_limit(root_msg_id), Ok(None));
    })
}

#[test]
fn test_imbalances_drop() {
    new_test_ext().execute_with(|| {
        let pos_imb = PositiveImbalance::<Test>::new(100);
        assert_eq!(super::TotalIssuance::<Test>::get(), 0);
        drop(pos_imb);
        assert_eq!(super::TotalIssuance::<Test>::get(), 100);
        let neg_imb = NegativeImbalance::<Test>::new(50);
        assert_eq!(super::TotalIssuance::<Test>::get(), 100);
        let new_neg = NegativeImbalance::<Test>::new(30).merge(neg_imb);
        assert_eq!(super::TotalIssuance::<Test>::get(), 100);
        drop(new_neg);
        assert_eq!(super::TotalIssuance::<Test>::get(), 20);
    })
}

#[test]
fn catch_value_all_blocked() {
    new_test_ext().execute_with(|| {
        // All nodes are blocked
        let root = H256::random();
        let spec_1 = H256::random();
        let spec_2 = H256::random();
        let spec_3 = H256::random();

        let res = Gas::create(H256::random(), root, 10000);
        assert!(res.is_ok());
        drop(res);
        assert_eq!(Gas::total_supply(), 10000);
        assert_ok!(Gas::split(root, H256::random()));
        assert_ok!(Gas::split(root, H256::random()));

        assert_ok!(Gas::split_with_value(root, spec_1, 100));
        assert_ok!(Gas::split(spec_1, H256::random()));
        assert_ok!(Gas::split(spec_1, H256::random()));

        assert_ok!(Gas::split_with_value(root, spec_2, 100));
        assert_ok!(Gas::split(spec_2, H256::random()));
        assert_ok!(Gas::split(spec_2, H256::random()));

        assert_ok!(Gas::split_with_value(root, spec_3, 100));
        assert_ok!(Gas::split(spec_3, H256::random()));
        assert_ok!(Gas::split(spec_3, H256::random()));

        assert!(matches!(Gas::consume(root).unwrap(), None));
        assert!(matches!(Gas::consume(spec_1).unwrap(), None));
        assert!(matches!(Gas::consume(spec_2).unwrap(), None));
        assert!(matches!(Gas::consume(spec_3).unwrap(), None));

        assert_eq!(Gas::total_supply(), 10000);
    })
}

#[test]
fn catch_value_all_catch() {
    new_test_ext().execute_with(|| {
        // All nodes are blocked
        let root = H256::random();
        let spec_1 = H256::random();
        let spec_2 = H256::random();
        let spec_3 = H256::random();

        let res = Gas::create(H256::random(), root, 10000);
        assert!(res.is_ok());
        drop(res);
        assert_eq!(Gas::total_supply(), 10000);
        assert_ok!(Gas::split_with_value(root, spec_1, 100));
        assert_ok!(Gas::split_with_value(root, spec_2, 100));
        assert_ok!(Gas::split_with_value(root, spec_3, 100));

        assert!(matches!(Gas::consume(root).unwrap(), Some(_)));
        assert!(matches!(Gas::consume(spec_1).unwrap(), Some(_)));
        assert!(matches!(Gas::consume(spec_2).unwrap(), Some(_)));
        assert!(matches!(Gas::consume(spec_3).unwrap(), Some(_)));

        assert_eq!(Gas::total_supply(), 0);
    })
}
