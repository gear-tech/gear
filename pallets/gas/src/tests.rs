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

type Gas = Pallet<Test>;

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

#[test]
fn gas_free_after_consumed() {
    sp_io::TestExternalities::new_empty().execute_with(|| {
        let origin = H256::random();
        let root_msg_id = H256::random();

        assert_ok!(Gas::create(origin, root_msg_id, 1000));
        assert_ok!(Gas::spend(root_msg_id, 300));

        let (v, _) = Gas::consume(root_msg_id).unwrap().unwrap();
        assert_eq!(v.peek(), 700);
        assert_eq!(Gas::get_limit(root_msg_id), Ok(None));
    })
}
