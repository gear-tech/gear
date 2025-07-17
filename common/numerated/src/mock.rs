// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

//! Mock for crate property testing and also can be used in other crates for their numerated types impls.

use crate::{
    iterators::IntervalIterator,
    numerated::{Bound, Numerated},
    tree::IntervalsTree,
};
use alloc::{collections::BTreeSet, fmt::Debug, vec::Vec};
use core::ops::Range;
use num_traits::{One, Zero, bounds::UpperBounded};
use proptest::{
    collection,
    prelude::{Arbitrary, any},
    prop_oneof,
    strategy::{BoxedStrategy, Strategy},
};

/// Mock function for any [`Numerated`] implementation testing.
pub fn test_numerated<T>(x: T, y: T)
where
    T: Numerated + Debug,
    T::Distance: Debug,
{
    assert_eq!(x.add_if_enclosed_by(T::Distance::one(), y), x.inc_if_lt(y));
    assert_eq!(x.sub_if_enclosed_by(T::Distance::one(), y), x.dec_if_gt(y));
    assert_eq!(y.add_if_enclosed_by(T::Distance::one(), x), y.inc_if_lt(x));
    assert_eq!(y.sub_if_enclosed_by(T::Distance::one(), x), y.dec_if_gt(x));

    assert_eq!(x.add_if_enclosed_by(T::Distance::zero(), y), Some(x));
    assert_eq!(x.sub_if_enclosed_by(T::Distance::zero(), y), Some(x));
    assert_eq!(y.add_if_enclosed_by(T::Distance::zero(), x), Some(y));
    assert_eq!(y.sub_if_enclosed_by(T::Distance::zero(), x), Some(y));

    let (x, y) = (x.min(y), x.max(y));
    if x == y {
        assert_eq!(x.inc_if_lt(y), None);
        assert_eq!(x.dec_if_gt(y), None);
        assert_eq!(x.distance(y), T::Distance::zero());
        assert_eq!(y.distance(x), T::Distance::zero());
    } else {
        let inc_x = x.inc_if_lt(y).unwrap();
        assert!(x.distance(inc_x) == T::Distance::one());
        assert!(x.dec_if_gt(y).is_none());

        let dec_y = y.dec_if_gt(x).unwrap();
        assert!(y.distance(dec_y) == T::Distance::one());
        assert!(y.inc_if_lt(y).is_none());

        let d = y.distance(x);
        assert_eq!(d, x.distance(y));
        assert_eq!(x.add_if_enclosed_by(d, y), Some(y));
        assert_eq!(y.sub_if_enclosed_by(d, x), Some(x));
    }
}

/// [`IntervalIterator`] testing action.
#[derive(Debug)]
pub enum IntervalAction<T: Numerated> {
    /// Try to create interval from correct start..end.
    Correct(T::Bound, T::Bound),
    /// Try to create interval from incorrect start..end.
    Incorrect(T::Bound, T::Bound),
}

/// Mock function for [`IntervalIterator`] testing for any [`Numerated`] implementation.
pub fn test_interval<T>(action: IntervalAction<T>)
where
    T: Numerated + UpperBounded + Debug,
    T::Bound: Debug,
{
    log::debug!("{action:?}");
    match action {
        IntervalAction::Incorrect(start, end) => {
            assert!(IntervalIterator::<T>::try_from(start..end).is_err());
            assert!(IntervalIterator::<T>::try_from((start, end)).is_err());
        }
        IntervalAction::Correct(start, end) => {
            let i = IntervalIterator::<T>::try_from(start..end).unwrap();
            assert_eq!(i, IntervalIterator::<T>::try_from((start, end)).unwrap());
            if start.unbound() == end.unbound() {
                assert!(i.is_empty());
                assert_eq!(i.inner(), None);
            } else {
                assert!(!i.is_empty());
                let i = i.inner().unwrap();
                assert_eq!(i.start(), start.unbound().unwrap());
                match end.unbound() {
                    Some(e) => assert_eq!(i.end(), e.dec_if_gt(i.start()).unwrap()),
                    None => assert_eq!(i.end(), T::max_value()),
                }
            }
        }
    }
}

/// [`IntervalsTree`] testing action.
#[derive(Debug)]
pub enum TreeAction<T> {
    /// Inserts interval into tree action.
    Insert(IntervalIterator<T>),
    /// Removes interval from tree action.
    Remove(IntervalIterator<T>),
    /// Check voids iterator.
    Voids(IntervalIterator<T>),
    /// Check difference iterator.
    Difference(BTreeSet<T>),
}

fn btree_set_voids<T: Numerated>(set: &BTreeSet<T>, interval: IntervalIterator<T>) -> BTreeSet<T> {
    interval.filter(|p| !set.contains(p)).collect()
}

/// Mock function for [`IntervalsTree`] testing for any [`Numerated`] implementation.
pub fn test_tree<T: Numerated + Debug>(initial: BTreeSet<T>, actions: Vec<TreeAction<T>>) {
    let mut tree: IntervalsTree<T> = initial.iter().copied().collect();
    let mut expected: BTreeSet<T> = tree.points_iter().collect();
    assert_eq!(expected, initial);

    for action in actions {
        log::debug!("{action:?}");
        match action {
            TreeAction::Insert(interval) => {
                tree.remove(interval);
                interval.for_each(|i| {
                    expected.remove(&i);
                });
            }
            TreeAction::Remove(interval) => {
                tree.insert(interval);
                expected.extend(interval);
            }
            TreeAction::Voids(interval) => {
                let voids: BTreeSet<T> = tree.voids(interval).flat_map(|i| i.iter()).collect();
                assert_eq!(voids, btree_set_voids(&expected, interval));
            }
            TreeAction::Difference(x) => {
                let y = x.iter().copied().collect();
                let z: BTreeSet<T> = tree.difference(&y).flat_map(|i| i.iter()).collect();
                assert_eq!(z, expected.difference(&x).copied().collect());
            }
        }
        assert_eq!(expected, tree.points_iter().collect());
    }
}

impl<T> Arbitrary for IntervalIterator<T>
where
    T: Numerated + Arbitrary + 'static,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        any::<(T, T)>()
            .prop_map(|(p1, p2)| (p1.min(p2)..=p1.max(p2)).try_into().unwrap())
            .boxed()
    }
}

impl<T> Arbitrary for TreeAction<T>
where
    T: Numerated + Arbitrary + 'static,
{
    type Parameters = Range<usize>;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(range: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            any::<IntervalIterator<T>>().prop_map(TreeAction::Insert),
            any::<IntervalIterator<T>>().prop_map(TreeAction::Remove),
            any::<IntervalIterator<T>>().prop_map(TreeAction::Voids),
            collection::btree_set(any::<T>(), range).prop_map(TreeAction::Difference),
        ]
        .boxed()
    }
}

impl<T> Arbitrary for IntervalAction<T>
where
    T: Numerated + Arbitrary + 'static,
    T::Bound: Debug,
{
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        let start = any::<Option<T>>();
        let end = any::<Option<T>>();
        (start, end)
            .prop_map(|(start, end)| {
                let start: T::Bound = start.into();
                let end: T::Bound = end.into();
                match (start.unbound(), end.unbound()) {
                    (_, None) => IntervalAction::Correct(start, end),
                    (Some(s), Some(e)) if s <= e => IntervalAction::Correct(start, end),
                    (Some(_), Some(_)) => IntervalAction::Incorrect(start, end),
                    (None, Some(_)) => IntervalAction::Incorrect(start, end),
                }
            })
            .boxed()
    }
}

/// Tree actions strategy.
pub fn tree_actions<T: Numerated + Arbitrary + 'static>(
    tree_size_range: Range<usize>,
    actions_amount_range: Range<usize>,
) -> impl Strategy<Value = (BTreeSet<T>, Vec<TreeAction<T>>)> {
    let action = TreeAction::arbitrary_with(tree_size_range.clone());
    (
        collection::btree_set(any::<T>(), tree_size_range),
        collection::vec(action, actions_amount_range),
    )
}
