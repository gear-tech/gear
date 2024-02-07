// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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
    Bound, IntervalIterator, Numerated, OptionBound,
};
use alloc::{collections::BTreeSet, fmt::Debug, vec::Vec};
use num_traits::bounds::{LowerBounded, UpperBounded};
use proptest::{
    arbitrary::{any, Arbitrary},
    prop_oneof, proptest,
    strategy::Strategy,
    test_runner::Config as ProptestConfig,
};

struct Generator<T>(T);

impl<T> Generator<T>
where
    T: Numerated + Arbitrary + Debug + LowerBounded + UpperBounded,
    T::Bound: Debug,
{
    fn rand_interval() -> impl Strategy<Value = IntervalIterator<T>> {
        any::<(T, T)>().prop_map(|(p1, p2)| (p1.min(p2)..=p1.max(p2)).try_into().unwrap())
    }

    fn interval_action() -> impl Strategy<Value = IntervalAction<T>> {
        let start = any::<Option<T>>();
        let end = any::<Option<T>>();
        (start, end).prop_map(|(start, end)| {
            let start: T::Bound = start.into();
            let end: T::Bound = end.into();
            match (start.unbound(), end.unbound()) {
                (_, None) => IntervalAction::Correct(start, end),
                (Some(s), Some(e)) if s <= e => IntervalAction::Correct(start, end),
                (Some(_), Some(_)) => IntervalAction::Incorrect(start, end),
                (None, Some(_)) => IntervalAction::Incorrect(start, end),
            }
        })
    }

    fn rand_set() -> impl Strategy<Value = BTreeSet<T>> {
        proptest::collection::btree_set(any::<T>(), 0..1000)
    }

    fn tree_actions() -> impl Strategy<Value = Vec<TreeAction<T>>> {
        let action = prop_oneof![
            Self::rand_interval().prop_map(TreeAction::Insert),
            Self::rand_interval().prop_map(TreeAction::Remove),
            Self::rand_interval().prop_map(TreeAction::Voids),
            Self::rand_set().prop_map(TreeAction::Difference),
        ];
        proptest::collection::vec(action, 10..20)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn proptest_numerated_i16(x in any::<i16>(), y in any::<i16>()) {
        mock::test_numerated(x, y);
    }

    #[test]
    fn proptest_interval_i16(action in Generator::<i16>::interval_action()) {
        mock::test_interval(action);
    }

    #[test]
    fn proptest_numerated_u16(x in any::<u16>(), y in any::<u16>()) {
        mock::test_numerated(x, y);
    }

    #[test]
    fn proptest_interval_u16(action in Generator::<u16>::interval_action()) {
        mock::test_interval(action);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn proptest_tree_i16(actions in Generator::<i16>::tree_actions(), initial in Generator::<i16>::rand_set()) {
        mock::test_tree(initial, actions);
    }

    #[test]
    fn proptest_tree_u16(actions in Generator::<u16>::tree_actions(), initial in Generator::<u16>::rand_set()) {
        mock::test_tree(initial, actions);
    }
}
