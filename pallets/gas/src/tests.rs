// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    gas_provider::{GasNodeId, Imbalance, NegativeImbalance},
    GasTree as _, LockId, LockableTree as _, Origin,
};
use frame_support::{assert_noop, assert_ok};
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
        let new_root = random_node_id();

        let pos = Gas::create(ALICE, new_root, 1000).unwrap();

        assert_eq!(pos.peek(), 1000);
        assert_eq!(Gas::total_supply(), 1000);

        let (_neg, owner) = Gas::consume(new_root).unwrap().unwrap();

        assert_eq!(owner, ALICE);
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
        assert_eq!(Gas::total_supply(), 300);
        assert_eq!(pos_imb.peek(), 300);
        // Chain of nodes, that form more likely a path rather then a tree
        assert_ok!(Gas::split_with_value(root, node_1, 200));
        assert_ok!(Gas::split_with_value(root, node_2, 100));
        assert_ok!(Gas::split_with_value(node_1, node_3, 100));
        assert_ok!(Gas::split(node_3, node_4));

        // Consume root
        let consume_root = Gas::consume(root);
        assert_eq!(consume_root.unwrap().unwrap().0.peek(), 0);
        // total supply mustn't be affected, because root sponsored all it's balance
        assert_eq!(Gas::total_supply(), 300);
        // Consumed still exists, but has no balance.
        assert_ok!(Gas::get_limit_node_consumed(root), (0, root.into()));

        // Consume node_1
        let consume_node_1 = Gas::consume(node_1);
        assert!(consume_node_1.is_ok());
        // Consumed node without unspec refs returns value,
        assert_eq!(consume_node_1.unwrap().unwrap().0.peek(), 100);
        // So it has no balance, but exists due to having children
        assert_ok!(Gas::get_limit_node_consumed(node_1), (0, node_1.into()));
        // total supply is affected
        assert_eq!(Gas::total_supply(), 200);
        // Check value wasn't moved up to the root
        assert_ok!(Gas::get_limit_node_consumed(root), (0, root.into()));

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
        assert_ok!(Gas::get_limit_node_consumed(node_3), (100, node_3.into()));
        // Check that spending from unspec `node_4` actually decreases balance from the ancestor with value - `node_3`.
        assert_eq!(Gas::spend(node_4, 100).unwrap().peek(), 100);
        assert_ok!(Gas::get_limit_node_consumed(node_3), (0, node_3.into()));
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
        Gas::create(ALICE, root, total_supply).unwrap();
        assert_ok!(Gas::cut(root, cut_a, cut_a_value));
        assert_ok!(Gas::split_with_value(root, specified, specified_value));
        assert_ok!(Gas::cut(specified, cut_b, cut_b_value));
        assert_ok!(Gas::split(root, unspecified));
        assert_ok!(Gas::cut(unspecified, cut_c, cut_c_value));

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
    let _ = env_logger::try_init();
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
        Gas::create(ALICE, root, total_supply).unwrap();
        assert_ok!(Gas::cut(root, cut, cut_value));
        assert_ok!(Gas::split_with_value(root, specified, specified_value));
        assert_ok!(Gas::split(root, unspecified));

        assert_eq!(Gas::total_supply(), total_supply);

        // consume nodes
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
        Gas::create(origin, root, 1000).unwrap();
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
        let split_3 = random_node_id();
        let cut = random_node_id();
        let cut_1 = random_node_id();

        let pos_imb = Gas::create(origin, new_root, 1000).unwrap();
        assert_eq!(Gas::total_supply(), 1000);
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
        assert_eq!(Gas::total_supply(), 800);

        assert_ok!(Gas::get_limit_node(new_root), (700, new_root.into()));
        // Try to split the reserved node
        assert_noop!(Gas::split(cut, split_1), Error::<Test>::Forbidden);

        // Try to split the reserved node with value
        assert_noop!(
            Gas::split_with_value(cut, split_3, 50),
            Error::<Test>::Forbidden
        );

        // Try to cut the reserved node
        assert_noop!(Gas::cut(cut, cut_1, 50), Error::<Test>::Forbidden);

        // Consume node.
        //
        // NOTE: root can't be consumed until it has unspec refs.
        assert_ok!(Gas::consume(split_1), None);
        assert_ok!(Gas::consume(split_2), None);
        assert!(Gas::consume(new_root).unwrap().is_some());
        // 100 is from the cut node
        assert_eq!(Gas::total_supply(), 100);
    });
}

#[test]
fn sub_nodes_tree_with_spends() {
    new_test_ext().execute_with(|| {
        let new_root = random_node_id();
        let origin = ALICE;
        let split_1 = random_node_id();
        let split_2 = random_node_id();

        Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split_with_value(new_root, split_1, 500));
        assert_ok!(Gas::split_with_value(new_root, split_2, 500));

        // Because root is not consumed, it is considered as a patron
        assert_ok!(Gas::consume(split_1), None);
        assert_ok!(Gas::consume(split_2), None);

        assert_eq!(Gas::total_supply(), 1000);
    });
}

#[test]
fn all_keys_are_cleared() {
    new_test_ext().execute_with(|| {
        let root = random_node_id();
        let origin = ALICE;
        let sub_keys = (0..5).map(|_| random_node_id()).collect::<Vec<_>>();

        Gas::create(origin, root, 2000).unwrap();
        for key in sub_keys.iter() {
            Gas::split_with_value(root, *key, 100).unwrap();
        }

        assert!(Gas::consume(root).unwrap().is_some());
        for key in sub_keys.iter() {
            // here we have not yet consumed everything
            assert!(GasTree::contains_key(GasNodeId::Node(*key)));

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

        Gas::create(origin, new_root, 1000).unwrap();

        assert_ok!(Gas::split(new_root, split_1));
        assert_ok!(Gas::split(new_root, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_2, 500));

        assert_eq!(Gas::spend(split_1, 200).unwrap().peek(), 200);

        assert_ok!(Gas::consume(split_1), None);
        assert_ok!(Gas::consume(split_2), None);

        // gas-less nodes are always leaves, so easily removed
        assert_noop!(Gas::get_external(split_1), Error::<Test>::NodeNotFound);
        assert_noop!(Gas::get_external(split_2), Error::<Test>::NodeNotFound);

        // Returns None, because root is not consumed, so considered as a patron
        assert_ok!(Gas::consume(split_1_2), None);

        let final_imb = Gas::consume(new_root).unwrap().unwrap().0;
        assert_eq!(final_imb.peek(), 800);

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

        Gas::create(origin, root, 2000).unwrap();

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
        assert_ok!(
            Gas::get_limit_node(root),
            (root_expected_limit, root.into())
        );
        assert_ok!(Gas::get_limit_node(m1), (m1_expected_limit, m1.into()));
        assert_ok!(Gas::get_limit_node(m2), (m2_expected_limit, m2.into()));
        assert_ok!(Gas::get_limit_node(m3), (200, m3.into()));
        assert_ok!(Gas::get_limit_node(m4), (200, m4.into()));

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

        Gas::create(origin, root_node, 1100).unwrap();

        assert_ok!(Gas::cut(root_node, cut, 300));
        assert_ok!(Gas::split(root_node, split_1));
        assert_ok!(Gas::split(root_node, split_2));
        assert_ok!(Gas::split_with_value(split_1, split_1_1, 600));
        assert_ok!(Gas::split(split_1, split_1_2));
        assert_ok!(Gas::split(split_1_1, split_1_1_1));

        // Original 1100 less 200 that were `cut` and `split_with_value`
        assert_ok!(Gas::get_limit_node(root_node), (200, root_node.into()));

        // 300 cut from the root node
        assert_ok!(Gas::get_limit_node(cut), (300, cut.into()));

        // Parent's 200
        assert_ok!(Gas::get_limit_node(split_1), (200, root_node.into()));

        // Parent's 200
        assert_ok!(Gas::get_limit_node(split_2), (200, root_node.into()));

        // Proprietary 600
        assert_ok!(Gas::get_limit_node(split_1_1), (600, split_1_1.into()));

        // Grand-parent's 200
        assert_ok!(Gas::get_limit_node(split_1_2), (200, root_node.into()));

        // Parent's 600
        assert_ok!(Gas::get_limit_node(split_1_1_1), (600, split_1_1.into()));

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
        Gas::create(origin, root, 1000).unwrap();
        assert_ok!(Gas::split_with_value(root, node_1, 800));
        assert_ok!(Gas::split_with_value(node_1, node_2, 500));
        assert_ok!(Gas::split(node_1, node_3));
        assert_ok!(Gas::split(node_2, node_4));
        assert_ok!(Gas::split_with_value(node_2, node_5, 250));

        // Check gas limits in the beginning
        assert_ok!(Gas::get_limit_node(root), (200, root.into()));
        assert_ok!(Gas::get_limit_node(node_1), (300, node_1.into()));
        assert_ok!(Gas::get_limit_node(node_2), (250, node_2.into()));
        // defined by parent
        assert_ok!(Gas::get_limit_node(node_3), (300, node_1.into()));
        // defined by parent
        assert_ok!(Gas::get_limit_node(node_4), (250, node_2.into()));
        assert_ok!(Gas::get_limit_node(node_5), (250, node_5.into()));

        // Consume node_1
        assert!(Gas::consume(node_1).unwrap().is_none());
        // Expect gas limit of the node_3 to remain unchanged
        assert_ok!(Gas::get_limit(node_3), 300);

        // Consume node_2
        assert!(Gas::consume(node_2).unwrap().is_none());
        // Marked as consumed
        assert!(GasTree::get(GasNodeId::Node(node_2))
            .map(|node| node.is_consumed())
            .unwrap());
        // Expect gas limit of the node_4 to remain unchanged
        assert_ok!(Gas::get_limit(node_4), 250);

        // Consume node 5
        assert!(Gas::consume(node_5).unwrap().is_none());
        // node_5 was removed
        assert_noop!(Gas::get_limit_node(node_5), Error::<Test>::NodeNotFound);
        // Expect gas limit from node_5 sent to upstream node with a value (node_2, which is consumed)
        assert_ok!(Gas::get_limit_consumed(node_2), 500);

        // Spend from unspecified node_4, which actually spends gas from node_2 (ancestor with value)
        assert_ok!(Gas::spend(node_4, 200));
        // Expect gas limit of consumed node_2 to decrease by 200 (thus checking we can spend from consumed node)
        assert_ok!(Gas::get_limit_consumed(node_2), 300);
        // Or explicitly spend from consumed node_2 by calling "spend"
        assert_ok!(Gas::spend(node_2, 200));
        assert_ok!(Gas::get_limit_consumed(node_2), 100);
    });
}

#[test]
fn gas_free_after_consumed() {
    new_test_ext().execute_with(|| {
        let origin = BOB;
        let root_msg_id = random_node_id();

        Gas::create(origin, root_msg_id, 1000).unwrap();
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

#[test]
fn lock_works() {
    new_test_ext().execute_with(|| {
        // Ids for nodes declaration.
        let external = random_node_id();
        let specified = random_node_id();
        let unspecified = random_node_id();
        let reserved = random_node_id();

        // Creating external and locking some value.
        Gas::create(ALICE, external, 10_000).unwrap();
        assert_eq!(Gas::total_supply(), 10_000);

        // Lock value for book a slot in waitlist
        assert_ok!(Gas::lock(external, LockId::Waitlist, 700));

        assert_eq!(Gas::total_supply(), 10_000);
        // Lock for the waitlist has value
        assert_ok!(Gas::get_lock(external, LockId::Waitlist), 700);
        // Other lock types are not used
        assert_ok!(Gas::get_lock(external, LockId::Mailbox), 0);
        assert_ok!(Gas::get_lock(external, LockId::Reservation), 0);
        assert_ok!(Gas::get_lock(external, LockId::DispatchStash), 0);

        assert_ok!(Gas::get_limit(external), 9_300);

        // Creating specified by root and locking some value.
        assert_ok!(Gas::split_with_value(external, specified, 3_000));

        assert_eq!(Gas::total_supply(), 10_000);
        assert_ok!(Gas::get_limit(external), 6_300);
        // Lock gas for paying for mailbox
        assert_ok!(Gas::lock(specified, LockId::Mailbox, 600));

        assert_eq!(Gas::total_supply(), 10_000);
        assert_ok!(Gas::get_lock(specified, LockId::Mailbox), 600);
        assert_ok!(Gas::get_lock(specified, LockId::Waitlist), 0);
        assert_ok!(Gas::get_limit(specified), 2_400);

        // Creating reserved node from root and trying to lock some value there,
        // consuming it afterward.
        assert_ok!(Gas::cut(external, reserved, 1_000));

        assert_eq!(Gas::total_supply(), 10_000);
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 0);
        assert_ok!(Gas::lock(reserved, LockId::Reservation, 500));
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 500);
        assert_ok!(Gas::lock(reserved, LockId::Reservation, 300));
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 800);
        assert_ok!(Gas::lock(reserved, LockId::Mailbox, 200));
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 800);
        assert_ok!(Gas::unlock(reserved, LockId::Reservation, 500));
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 300);
        assert_ok!(Gas::unlock(reserved, LockId::Reservation, 300));
        assert_ok!(Gas::get_lock(reserved, LockId::Reservation), 0);

        // `reserved` node still has a lock on it
        assert_noop!(Gas::consume(reserved), Error::<Test>::ConsumedWithLock);
        // release the remaining lock
        assert_eq!(Gas::unlock_all(reserved, LockId::Mailbox).unwrap(), 200);

        // Now the `reserved` node can be consumed
        let neg_imb = Gas::consume(reserved).unwrap().unwrap();
        assert_eq!(Gas::total_supply(), 9_000);
        assert_eq!(neg_imb.0.peek(), 1_000);

        // Unlocking part of locked value on specified node.
        assert_ok!(Gas::unlock(specified, LockId::Mailbox, 500));

        assert_eq!(Gas::total_supply(), 9_000);
        assert_ok!(Gas::get_lock(specified, LockId::Mailbox), 100);
        assert_ok!(Gas::get_limit(specified), 2_900);

        // Creating unspecified node from specified one,
        // locking value there afterward.
        assert_ok!(Gas::split(specified, unspecified));

        assert_ok!(Gas::get_lock(unspecified, LockId::Waitlist), 0);

        assert_ok!(Gas::lock(unspecified, LockId::Waitlist, 600));

        assert_eq!(Gas::total_supply(), 9_000);
        assert_ok!(Gas::get_lock(specified, LockId::Mailbox), 100);
        assert_ok!(Gas::get_limit(specified), 2_300);
        assert_ok!(Gas::get_lock(unspecified, LockId::Waitlist), 600);
        assert_ok!(Gas::get_limit(unspecified), 2_300);

        // Trying to consume specified, while lock exists.
        assert_noop!(Gas::consume(specified), Error::<Test>::ConsumedWithLock);

        // Trying to unlock greater value than we have locked.
        assert_noop!(
            Gas::unlock(specified, LockId::Mailbox, 101),
            Error::<Test>::InsufficientBalance
        );

        // Success unlock for full and consuming of specified node
        // (unspecified from it still exists).
        assert_ok!(Gas::unlock(specified, LockId::Mailbox, 100));

        assert_ok!(Gas::consume(specified), None);

        assert_noop!(
            Gas::lock(specified, LockId::Waitlist, 1),
            Error::<Test>::NodeWasConsumed
        );
        assert_noop!(
            Gas::unlock(specified, LockId::Waitlist, 1),
            Error::<Test>::NodeWasConsumed
        );

        assert_eq!(Gas::total_supply(), 9_000);
        assert_ok!(Gas::get_lock(specified, LockId::Mailbox), 0);
        assert_ok!(Gas::get_limit_consumed(specified), 2_400);
        assert_ok!(Gas::get_lock(unspecified, LockId::Waitlist), 600);
        assert_ok!(Gas::get_limit(unspecified), 2_400);

        // Unlocking and consuming unspecified.
        assert_ok!(Gas::unlock(unspecified, LockId::Waitlist, 600));

        assert_ok!(Gas::consume(unspecified), None);

        assert_ok!(Gas::unlock(external, LockId::Waitlist, 700));

        // Finally free all supply by consuming root.
        let neg_imb = Gas::consume(external).unwrap().unwrap();
        assert_eq!(Gas::total_supply(), 0);
        assert_eq!(neg_imb.0.peek(), 9_000);
    })
}
