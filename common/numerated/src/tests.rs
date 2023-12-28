// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Property testing for Numerated, Interval and IntervalsTree.

use crate::{
    mock::{self, IntervalAction, TreeAction},
    Bound, IntervalIterator, OptionBound,
};
use alloc::{collections::BTreeSet, vec::Vec};
use proptest::{
    arbitrary::any, prop_oneof, proptest, strategy::Strategy, test_runner::Config as ProptestConfig,
};

macro_rules! any_numerated {
    ($t:ty) => {
        fn rand_interval() -> impl Strategy<Value = IntervalIterator<$t>> {
            any::<$t>().prop_flat_map(|start| {
                (start..).prop_map(move |end| (start..=end).try_into().unwrap())
            })
        }

        fn rand_set() -> impl Strategy<Value = BTreeSet<$t>> {
            proptest::collection::btree_set(any::<$t>(), 0..1000)
        }

        fn tree_actions() -> impl Strategy<Value = Vec<TreeAction<$t>>> {
            let action = prop_oneof![
                rand_interval().prop_map(TreeAction::Insert),
                rand_interval().prop_map(TreeAction::Remove),
                rand_interval().prop_map(TreeAction::Voids),
                rand_set().prop_map(TreeAction::Difference),
            ];
            proptest::collection::vec(action, 10..20)
        }

        fn interval_action() -> impl Strategy<Value = IntervalAction<$t>> {
            let start = any::<Option<$t>>();
            let end = any::<Option<$t>>();
            (start, end).prop_map(|(start, end)| {
                let start: OptionBound<$t> = start.into();
                let end: OptionBound<$t> = end.into();
                match (start.unbound(), end.unbound()) {
                    (_, None) => IntervalAction::Correct(start, end),
                    (Some(s), Some(e)) if s <= e => IntervalAction::Correct(start, end),
                    (Some(_), Some(_)) => IntervalAction::Incorrect(start, end),
                    (None, Some(_)) => IntervalAction::Incorrect(start, end),
                }
            })
        }
    };
}

mod test_i16 {
    use super::*;

    any_numerated!(i16);

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        #[test]
        fn proptest_numerated(x in any::<i16>(), y in any::<i16>()) {
            mock::test_numerated(x, y);
        }

        #[test]
        fn proptest_interval(action in interval_action()) {
            mock::test_interval(action);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn proptest_tree(actions in tree_actions(), initial in rand_set()) {
            mock::test_tree(initial, actions);
        }
    }
}

mod test_u16 {
    use super::*;

    any_numerated!(u16);

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        #[test]
        fn proptest_numerated(x in any::<u16>(), y in any::<u16>()) {
            mock::test_numerated(x, y);
        }

        #[test]
        fn proptest_interval(action in interval_action()) {
            mock::test_interval(action);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn proptest_tree(actions in tree_actions(), initial in rand_set()) {
            mock::test_tree(initial, actions);
        }
    }
}
