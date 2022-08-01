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
use common::{
    gas_provider::{NegativeImbalance, PositiveImbalance},
    GasTree as _, Origin,
};
use frame_support::{assert_noop, assert_ok, traits::Imbalance};
use gear_core::ids::MessageId;
use primitive_types::H256;
use sp_runtime::traits::Zero;

type Gas = <Pallet<Test> as common::GasProvider>::GasTree;
type GasTree = GasNodes<Test>;

fn random_node_id() -> MessageId {
    MessageId::from_origin(H256::random())
}

#[test]
fn simple_value_tree() {
    new_test_ext().execute_with(|| {
        let new_root = MessageId::from_origin(H256::random());

        {
            let pos = Gas::create(ALICE, new_root, 1000).unwrap();

            assert_eq!(pos.peek(), 1000);
            assert!(Gas::total_supply().is_zero());
        }
        // Positive imbalance dropped - the total issuance should have been upped to 1000
        assert_eq!(Gas::total_supply(), 1000);

        {
            let (_neg, owner) = Gas::consume(new_root).unwrap().unwrap();

            assert_eq!(owner, ALICE);
            assert_eq!(Gas::total_supply(), 1000);
        }
        // Total supply back to original value
        assert!(Gas::total_supply().is_zero());
    });
}

#[test]
fn test_consume_procedure_with_subnodes() {
    new_test_ext().execute_with(|| {
        let root = random_node_id();
        let node_1 = random_node_id();
        let node_2 = random_node_id();
        let node_3 = random_node_id();
        let node_4 = random_node_id();

        let pos_imb = Gas::create(ALICE, root, 300).unwrap();
        assert_eq!(pos_imb.peek(), 300);
        // Chain of nodes, that form more likely a path rather then a tree
        assert_ok!(Gas::split_with_value(root, node_1, 200));
        assert_ok!(Gas::split_with_value(root, node_2, 100));
        assert_ok!(Gas::split_with_value(node_1, node_3, 100));
        assert_ok!(Gas::split(node_3, node_4));

        assert!(Gas::total_supply().is_zero());

        // We must drop the imbalance to reflect changes in total supply
        drop(pos_imb);
        assert_eq!(Gas::total_supply(), 300);

        // Consume root
        let consume_root = Gas::consume(root);
        assert_eq!(consume_root.unwrap().unwrap().0.peek(), 0);
        // total supply mustn't be affected, because root sponsored all it's balance
        assert_eq!(Gas::total_supply(), 300);
        // Consumed still exists, but has no balance.
        assert_ok!(Gas::get_limit_node(root), (0, root));

        // Consume node_1
        let consume_node_1 = Gas::consume(node_1);
        assert!(consume_node_1.is_ok());
        // Consumed node without unspec refs returns value,
        assert_eq!(consume_node_1.unwrap().unwrap().0.peek(), 100);
        // So it has no balance, but exists due to having children
        assert_ok!(Gas::get_limit_node(node_1), (0, node_1));
        // total supply is affected
        assert_eq!(Gas::total_supply(), 200);
        // Check value wasn't moved up to the root
        assert_ok!(Gas::get_limit_node(root), (0, root));

        // Consume node_2 independently
        let consume_node_2 = Gas::consume(node_2);
        // The node is not a patron, so value should be returned
        assert_eq!(consume_node_2.unwrap().unwrap().0.peek(), 100);
        // It has no children, so should be removed
        assert_noop!(Gas::get_limit_node(node_2), Error::<Test>::NodeNotFound);
        // Total supply is affected
        assert_eq!(Gas::total_supply(), 100);

        // Consume node_3
        assert_ok!(Gas::consume(node_3), None);
        // Consumed node with unspec refs doesn't moves value up
        assert_ok!(Gas::get_limit_node(node_3), (100, node_3));
        // Check that spending from unspec `node_4` actually decreases balance from the ancestor with value - `node_3`.
        assert_eq!(Gas::spend(node_4, 100).unwrap().peek(), 100);
        assert_ok!(Gas::get_limit_node(node_3), (0, node_3));
        // total supply is affected after spending all of the blockage `node_3`
        assert!(Gas::total_supply().is_zero());
        // Still exists, although is consumed and has a zero balance. The only way to remove it is to remove children.
        assert_noop!(Gas::consume(node_3), Error::<Test>::NodeWasConsumed);

        // Impossible to consume non-existing node.
        assert_noop!(Gas::consume(random_node_id()), Error::<Test>::NodeNotFound);

        // Before consuming blockage `node_4`
        assert_ok!(Gas::get_external(root));
        assert_ok!(Gas::get_external(node_1));
        assert_ok!(Gas::get_external(node_3));

        let consume_node_4 = Gas::consume(node_4);
        assert_eq!(consume_node_4.unwrap().unwrap().0.peek(), 0);

        // After consuming blockage `node_3`
        assert!(GasTree::iter_keys().next().is_none());
    })
}

#[test]
fn can_cut_nodes() {
    new_test_ext().execute_with(|| {
        let (root, specified, unspecified, cut_a, cut_b, cut_c) = (
            random_node_id(),
            random_node_id(),
            random_node_id(),
            random_node_id(),
            random_node_id(),
            random_node_id(),
        );
        let (total_supply, specified_value, cut_a_value, cut_b_value, cut_c_value) =
            (1000, 500, 300, 200, 100);

        // create nodes
        {
            assert!(Gas::create(ALICE, root, total_supply).is_ok());
            assert_ok!(Gas::cut(root, cut_a, cut_a_value));
            assert_ok!(Gas::split_with_value(root, specified, specified_value));
            assert_ok!(Gas::cut(specified, cut_b, cut_b_value));
            assert_ok!(Gas::split(root, unspecified));
            assert_ok!(Gas::cut(unspecified, cut_c, cut_c_value));
        }

        assert_eq!(Gas::total_supply(), total_supply);

        let root_limit = total_supply - specified_value - cut_a_value - cut_c_value;
        assert_ok!(Gas::get_limit(root), root_limit);
        assert_ok!(Gas::get_limit(specified), specified_value - cut_b_value);
        assert_ok!(Gas::get_limit(cut_a), cut_a_value);
        assert_ok!(Gas::get_limit(cut_b), cut_b_value);
        assert_ok!(Gas::get_limit(cut_c), cut_c_value);
    })
}

#[test]
fn value_tree_with_all_kinds_of_nodes() {
    env_logger::init();
    new_test_ext().execute_with(|| {
        let total_supply = 1000;
        let cut_value = 300;
        let specified_value = total_supply - cut_value;
        let (root, cut, specified, unspecified) = (
            random_node_id(),
            random_node_id(),
            random_node_id(),
            random_node_id(),
        );

        // create nodes
        {
            assert!(Gas::create(ALICE, root, total_supply).is_ok());
            assert_ok!(Gas::cut(root, cut, cut_value));
            assert_ok!(Gas::split_with_value(root, specified, specified_value));
            assert_ok!(Gas::split(root, unspecified));
        }

        assert_eq!(Gas::total_supply(), total_supply);

        // consume nodes
        {
            assert_ok!(Gas::consume(unspecified), None);
            // Root is considered a patron, because is not consumed
            assert_ok!(Gas::consume(specified), None);
            assert_eq!(Gas::total_supply(), total_supply);

            assert_ok!(
                Gas::consume(root),
                Some((NegativeImbalance::new(specified_value), ALICE))
            );
            assert_ok!(
                Gas::consume(cut),
                Some((NegativeImbalance::new(cut_value), ALICE))
            );
        }

        assert!(Gas::total_supply().is_zero());
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
        let root = random_node_id();
        let node_1 = random_node_id();
        let node_2 = random_node_id();
        let node_3 = random_node_id();

        // Prepare the initial configuration
        assert!(Gas::create(origin, root, 1000).is_ok());
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));

        assert_ok!(Gas::consume(node_1), None);
        // Can actually split consumed
        assert_ok!(Gas::split(node_1, random_node_id()));

        // Can't split with existing id.
        assert_noop!(Gas::split(root, node_2), Error::<Test>::NodeAlreadyExists);
        // Special case is when provided 2 existing equal ids
        assert_noop!(Gas::split(node_2, node_2), Error::<Test>::NodeAlreadyExists);
        // Not equal ids can be caught as well
        let node_4 = random_node_id();
        assert_noop!(Gas::split(node_4, node_4), Error::<Test>::NodeNotFound);
    })
}

#[test]
fn value_tree_known_errors() {
    new_test_ext().execute_with(|| {
        let new_root = random_node_id();
        let origin = ALICE;
        let split_1 = random_node_id();
        let split_2 = random_node_id();
        let cut = random_node_id();
        let cut_1 = random_node_id();

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

            assert_ok!(Gas::get_limit_node(new_root), (700, new_root));
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
            assert!(Gas::total_supply().is_zero());

            // Consume node.
            //
            // NOTE: root can't be consumed until it has unspec refs.
            assert_ok!(Gas::consume(split_1), None);
            assert_ok!(Gas::consume(split_2), None);
            assert!(Gas::consume(new_root).unwrap().is_some());

            // Negative imbalance dropped immediately - total supply decreased (in saturating way)
            // In practice it means the total supply is still 0
            assert!(Gas::total_supply().is_zero());
        }
        // Now initial positive imbalance has been dropped
        // which should have affected the total supply
        assert_eq!(Gas::total_supply(), 1000);

        // TODO: dropping imbalances in wrong order can lead to incorrect total supply value
    });
}

#[test]
fn sub_nodes_tree_with_spends() {
    new_test_ext().execute_with(|| {
        let new_root = random_node_id();
        let origin = ALICE;
        let split_1 = random_node_id();
        let split_2 = random_node_id();

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split_with_value(new_root, split_1, 500));
        assert_ok!(Gas::split_with_value(new_root, split_2, 500));

        let offset1 = pos_imb
            .offset(Gas::spend(split_1, 100).unwrap())
            .same()
            .unwrap();

        assert_eq!(offset1.peek(), 900);

        // Because root is not consumed, it is considered as a patron
        assert_ok!(Gas::consume(split_1), None);
        assert_ok!(Gas::consume(split_2), None);

        let offset2 = offset1
            .offset(Gas::consume(new_root).unwrap().unwrap().0)
            .same()
            .unwrap();

        assert!(offset2.peek().is_zero());
        assert_ok!(offset2.drop_zero());

        assert!(Gas::total_supply().is_zero());
    });
}

#[test]
fn all_keys_are_cleared() {
    new_test_ext().execute_with(|| {
        let root = random_node_id();
        let origin = ALICE;
        let sub_keys = (0..5)
            .map(|_| MessageId::from_origin(H256::random()))
            .collect::<Vec<_>>();

        Gas::create(origin, root, 2000).unwrap();
        for key in sub_keys.iter() {
            Gas::split_with_value(root, *key, 100).unwrap();
        }

        assert!(Gas::consume(root).unwrap().is_some());
        for key in sub_keys.iter() {
            // here we have not yet consumed everything
            assert!(GasTree::contains_key(*key));

            // There are no patron nodes in the tree after root was consumed
            assert!(Gas::consume(*key).unwrap().is_some());
        }

        // here we consumed everything
        let key_count = GasTree::iter_keys().fold(0, |k, _| k + 1);
        assert_eq!(key_count, 0);
    });
}

#[test]
fn split_with_no_value() {
    new_test_ext().execute_with(|| {
        let new_root = random_node_id();
        let origin = ALICE;
        let split_1 = random_node_id();
        let split_2 = random_node_id();
        let split_1_2 = random_node_id();

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

        assert_ok!(Gas::consume(split_1), None);
        assert_ok!(Gas::consume(split_2), None);

        // gas-less nodes are always leaves, so easily removed
        assert_noop!(Gas::get_external(split_1), Error::<Test>::NodeNotFound);
        assert_noop!(Gas::get_external(split_2), Error::<Test>::NodeNotFound);

        // Returns None, because root is not consumed, so considered as a patron
        assert_ok!(Gas::consume(split_1_2), None);

        let final_imb = Gas::consume(new_root).unwrap().unwrap().0;
        assert_eq!(final_imb.peek(), 700);

        assert!(Gas::total_supply().is_zero());
    });
}

#[test]
fn long_chain() {
    new_test_ext().execute_with(|| {
        let root = random_node_id();
        let m1 = random_node_id();
        let m2 = random_node_id();
        let m3 = random_node_id();
        let m4 = random_node_id();
        let origin = ALICE;

        assert!(Gas::create(origin, root, 2000).is_ok());

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
        assert_ok!(Gas::get_limit_node(root), (root_expected_limit, root));
        assert_ok!(Gas::get_limit_node(m1), (m1_expected_limit, m1));
        assert_ok!(Gas::get_limit_node(m2), (m2_expected_limit, m2));
        assert_ok!(Gas::get_limit_node(m3), (200, m3));
        assert_ok!(Gas::get_limit_node(m4), (200, m4));

        // Send their value to the root, which is not consumed. therefore considered as a patron
        assert_ok!(Gas::consume(m1), None);
        assert_ok!(Gas::consume(m2), None);
        // Doesn't have any unspec refs. so not a patron
        assert!(Gas::consume(root).unwrap().is_some());
        // Has a patron parent m3
        assert_ok!(Gas::consume(m4), None);

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
    new_test_ext().execute_with(|| {
        let origin = BOB;
        let root_node = random_node_id();
        let cut = random_node_id();
        let split_1 = random_node_id();
        let split_2 = random_node_id();
        let split_1_1 = random_node_id();
        let split_1_2 = random_node_id();
        let split_1_1_1 = random_node_id();

        assert!(Gas::create(origin, root_node, 1100).is_ok());

        assert_ok!(Gas::cut(root_node, cut, 300));
        assert_ok!(Gas::split(root_node, split_1));
        assert_ok!(Gas::split(root_node, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_1, 600));
        assert_ok!(Gas::split(split_1, split_1_2));
        assert_ok!(Gas::split(split_1_1, split_1_1_1));

        // Original 1100 less 200 that were `cut` and `split_with_value`
        assert_ok!(Gas::get_limit_node(root_node), (200, root_node));

        // 300 cut from the root node
        assert_ok!(Gas::get_limit_node(cut), (300, cut));

        // Parent's 200
        assert_ok!(Gas::get_limit_node(split_1), (200, root_node));

        // Parent's 200
        assert_ok!(Gas::get_limit_node(split_2), (200, root_node));

        // Proprietary 600
        assert_ok!(Gas::get_limit_node(split_1_1), (600, split_1_1));

        // Grand-parent's 200
        assert_ok!(Gas::get_limit_node(split_1_2), (200, root_node));

        // Parent's 600
        assert_ok!(Gas::get_limit_node(split_1_1_1), (600, split_1_1));

        // All nodes origin is `origin`
        assert_ok!(Gas::get_external(root_node), origin);
        assert_ok!(Gas::get_external(cut), origin);
        assert_ok!(Gas::get_external(split_1), origin);
        assert_ok!(Gas::get_external(split_2), origin);
        assert_ok!(Gas::get_external(split_1_1), origin);
        assert_ok!(Gas::get_external(split_1_2), origin);
        assert_ok!(Gas::get_external(split_1_1_1), origin);
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
    new_test_ext().execute_with(|| {
        let origin = BOB;
        let root = random_node_id();
        let node_1 = random_node_id();
        let node_2 = random_node_id();
        let node_3 = random_node_id();
        let node_4 = random_node_id();
        let node_5 = random_node_id();

        // Prepare the initial configuration
        assert!(Gas::create(origin, root, 1000).is_ok());
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));
        assert_ok!(Gas::split(node_2, node_4));
        assert_ok!(Gas::split_with_value(node_2, node_5, 250));

        // Check gas limits in the beginning
        assert_ok!(Gas::get_limit_node(root), (200, root));
        assert_ok!(Gas::get_limit_node(node_1), (300, node_1));
        assert_ok!(Gas::get_limit_node(node_2), (250, node_2));
        // defined by parent
        assert_ok!(Gas::get_limit_node(node_3), (300, node_1));
        // defined by parent
        assert_ok!(Gas::get_limit_node(node_4), (250, node_2));
        assert_ok!(Gas::get_limit_node(node_5), (250, node_5));

        // Consume node_1
        assert!(Gas::consume(node_1).unwrap().is_none());
        // Expect gas limit of the node_3 to remain unchanged
        assert_ok!(Gas::get_limit(node_3), 300);

        // Consume node_2
        assert!(Gas::consume(node_2).unwrap().is_none());
        // Marked as consumed
        assert!(GasTree::get(node_2).map(|node| node.is_consumed()).unwrap());
        // Expect gas limit of the node_4 to remain unchanged
        assert_ok!(Gas::get_limit(node_4), 250);

        // Consume node 5
        assert!(Gas::consume(node_5).unwrap().is_none());
        // node_5 was removed
        assert_noop!(Gas::get_limit_node(node_5), Error::<Test>::NodeNotFound);
        // Expect gas limit from node_5 sent to upstream node with a value (node_2, which is consumed)
        assert_ok!(Gas::get_limit(node_2), 500);

        // Spend from unspecified node_4, which actually spends gas from node_2 (ancestor with value)
        assert_ok!(Gas::spend(node_4, 200));
        // Expect gas limit of consumed node_2 to decrease by 200 (thus checking we can spend from consumed node)
        assert_ok!(Gas::get_limit(node_2), 300);
        // Or explicitly spend from consumed node_2 by calling "spend"
        assert_ok!(Gas::spend(node_2, 200));
        assert_ok!(Gas::get_limit(node_2), 100);
    });
}

#[test]
fn gas_free_after_consumed() {
    new_test_ext().execute_with(|| {
        let origin = BOB;
        let root_msg_id = random_node_id();

        assert!(Gas::create(origin, root_msg_id, 1000).is_ok());
        assert_ok!(Gas::spend(root_msg_id, 300));

        let (v, _) = Gas::consume(root_msg_id).unwrap().unwrap();
        assert_eq!(v.peek(), 700);
        assert_noop!(
            Gas::get_limit_node(root_msg_id),
            Error::<Test>::NodeNotFound
        );
    })
}

#[test]
fn test_imbalances_drop() {
    new_test_ext().execute_with(|| {
        let pos_imb = PositiveImbalance::<Balance, TotalIssuanceWrap<Test>>::new(100);
        assert_eq!(TotalIssuance::<Test>::get(), None);
        drop(pos_imb);
        assert_eq!(TotalIssuance::<Test>::get(), Some(100));
        let neg_imb = NegativeImbalance::<Balance, TotalIssuanceWrap<Test>>::new(50);
        assert_eq!(TotalIssuance::<Test>::get(), Some(100));
        let new_neg = NegativeImbalance::<Balance, TotalIssuanceWrap<Test>>::new(30).merge(neg_imb);
        assert_eq!(TotalIssuance::<Test>::get(), Some(100));
        drop(new_neg);
        assert_eq!(TotalIssuance::<Test>::get(), Some(20));
    })
}

#[test]
fn catch_value_all_blocked() {
    new_test_ext().execute_with(|| {
        // All nodes are blocked
        let root = random_node_id();
        let spec_1 = random_node_id();
        let spec_2 = random_node_id();
        let spec_3 = random_node_id();

        Gas::create(ALICE, root, 10000).unwrap();
        assert_eq!(Gas::total_supply(), 10000);
        assert_ok!(Gas::split(root, random_node_id()));
        assert_ok!(Gas::split(root, random_node_id()));

        assert_ok!(Gas::split_with_value(root, spec_1, 100));
        assert_ok!(Gas::split(spec_1, random_node_id()));
        assert_ok!(Gas::split(spec_1, random_node_id()));

        assert_ok!(Gas::split_with_value(root, spec_2, 100));
        assert_ok!(Gas::split(spec_2, random_node_id()));
        assert_ok!(Gas::split(spec_2, random_node_id()));

        assert_ok!(Gas::split_with_value(root, spec_3, 100));
        assert_ok!(Gas::split(spec_3, random_node_id()));
        assert_ok!(Gas::split(spec_3, random_node_id()));

        // None of ops will catch the value
        assert!(Gas::consume(root).unwrap().is_none());
        assert!(Gas::consume(spec_1).unwrap().is_none());
        assert!(Gas::consume(spec_2).unwrap().is_none());
        assert!(Gas::consume(spec_3).unwrap().is_none());

        assert_eq!(Gas::total_supply(), 10000);
    })
}

#[test]
fn catch_value_all_catch() {
    new_test_ext().execute_with(|| {
        // All nodes are blocked
        let root = random_node_id();
        let spec_1 = random_node_id();
        let spec_2 = random_node_id();
        let spec_3 = random_node_id();

        Gas::create(ALICE, root, 10000).unwrap();
        assert_eq!(Gas::total_supply(), 10000);
        assert_ok!(Gas::split_with_value(root, spec_1, 100));
        assert_ok!(Gas::split_with_value(root, spec_2, 100));
        assert_ok!(Gas::split_with_value(root, spec_3, 100));

        assert!(Gas::consume(root).unwrap().is_some());
        assert!(Gas::consume(spec_1).unwrap().is_some());
        assert!(Gas::consume(spec_2).unwrap().is_some());
        assert!(Gas::consume(spec_3).unwrap().is_some());

        assert!(Gas::total_supply().is_zero());
    })
}
