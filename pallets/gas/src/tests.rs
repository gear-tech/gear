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

use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use primitive_types::H256;

type Gas = Pallet<Test>;

#[cfg(test)]
mod tests {
    use super::*;

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
                let (_neg, owner) = Gas::consume(new_root).unwrap();

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

            assert!(matches!(Gas::consume(new_root), None));
            assert!(matches!(Gas::consume(split_1), None));

            let consume_result = Gas::consume(split_2);
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
                Gas::consume(new_root);
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

            assert!(matches!(Gas::consume(split_1), None));
            assert!(matches!(Gas::consume(split_2), None));

            let offset2 = offset1
                .offset(Gas::consume(new_root).unwrap().0)
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

            Gas::consume(root);
            for key in sub_keys.iter() {
                // here we have not yet consumed everything
                assert!(ValueView::<Test>::contains_key(*key));

                Gas::consume(*key);
            }

            // here we consumed everything
            let key_count = ValueView::<Test>::iter_keys().fold(0, |k, _| k + 1);
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

            assert!(matches!(Gas::consume(split_1), None));
            assert!(matches!(Gas::consume(split_2), None));
            assert!(matches!(Gas::consume(split_1_2), None));

            let final_imb = Gas::consume(new_root).unwrap().0;
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

            assert!(matches!(Gas::consume(m1), None));
            assert!(matches!(Gas::consume(m2), None));
            assert!(matches!(Gas::consume(root), None));
            assert!(matches!(Gas::consume(m4), None));

            let (neg_imb, payee) = Gas::consume(m3).unwrap();

            // 2000 initial, 5*50 spent
            assert_eq!(neg_imb.peek(), 1750);
            assert_eq!(payee, origin);
        });
    }
}
