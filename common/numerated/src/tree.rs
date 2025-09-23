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

//! [`IntervalsTree`] implementation.

use crate::{
    interval::Interval,
    iterators::{DifferenceIterator, IntervalIterator, VoidsIterator},
    numerated::Numerated,
};
use alloc::{collections::BTreeMap, fmt, fmt::Debug, vec::Vec};
use core::{fmt::Formatter, ops::RangeInclusive};
use num_traits::{CheckedAdd, Zero};
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

/// # Non overlapping intervals tree
/// Can be considered as set of points, but with possibility to work with
/// continuous sets of this points (the same as interval) as fast as with points.
/// Insert and remove operations has complexity between `[O(log(n)), O(n)]`,
/// where `n` is amount of intervals in tree.
/// So, even if you insert for example points from `0u64` to `u64::MAX`,
/// then removing all of them or any part of them is as fast as removing one point.
///
/// # Examples
/// ```
/// use numerated::{interval::Interval, tree::IntervalsTree};
/// use std::{collections::BTreeSet, ops::RangeInclusive};
///
/// let mut tree = IntervalsTree::new();
/// let mut set = BTreeSet::new();
///
/// tree.insert(1i32);
/// // now `tree` contains only one interval: [1..=1]
/// set.insert(1i32);
/// // `points_iter()` - is iterator over all points in `tree`.
/// assert_eq!(set, tree.points_iter().collect());
///
/// // We can insert points from 3 to 100_000 only by one insert operation.
/// // `try` is only for range check, that it has start ≤ end.
/// // After `tree` will contain two intervals: `[1..=1]` and `[3..=100_000]`.
/// tree.insert(Interval::try_from(3..=100_000).unwrap());
/// // For `set` insert complexity == `O(n)`, where n is amount of elements in range.
/// set.extend(3..=100_000);
/// assert_eq!(set, tree.points_iter().collect());
///
/// // We can remove points from 1 to 99_000 (not inclusive) only by one remove operation.
/// // `try` is only for range check, that it has start ≤ end.
/// // After `tree` will contain two intervals: [99_000..=100_000]
/// tree.remove(Interval::try_from(1..99_000).unwrap());
/// // For `set` insert complexity == O(n*log(m)),
/// // where `n` is amount of elements in range and `m` is len of `set`.
/// (1..99_000).for_each(|i| {
///     set.remove(&i);
/// });
/// assert_eq!(set, tree.points_iter().collect());
///
/// // Can insert or remove all possible points just by one operation:
/// tree.insert(..);
/// tree.remove(..);
///
/// // Iterate over voids (intervals between intervals in tree):
/// tree.insert(Interval::try_from(1..=3).unwrap());
/// tree.insert(Interval::try_from(5..=7).unwrap());
/// let voids = tree.voids(..).map(RangeInclusive::from).collect::<Vec<_>>();
/// assert_eq!(voids, vec![i32::MIN..=0, 4..=4, 8..=i32::MAX]);
///
/// // Difference iterator: iterate over intervals from `tree` which are not in `other_tree`.
/// let other_tree: IntervalsTree<i32> = [3, 4, 5, 7, 8, 9].into_iter().collect();
/// let difference: Vec<_> = tree
///     .difference(&other_tree)
///     .map(RangeInclusive::from)
///     .collect();
/// assert_eq!(difference, vec![1..=2, 6..=6]);
/// ```
///
/// # Possible panic cases
/// Using `IntervalsTree` for type `T: Numerated` cannot cause panics,
/// if implementation [`Numerated`], [`Copy`], [`Ord`], [`Eq`] are correct for `T`.
/// In other cases `IntervalsTree` does not guarantees execution without panics.
#[derive(Clone, PartialEq, Eq, Hash, TypeInfo, Encode, Decode)]
#[codec(crate = scale_info::scale)]
pub struct IntervalsTree<T> {
    inner: BTreeMap<T, T>,
}

impl<T: Copy> IntervalsTree<T> {
    /// Creates new empty intervals tree.
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }
    /// Returns amount of not empty intervals in tree.
    ///
    /// Complexity: O(1).
    pub fn intervals_amount(&self) -> usize {
        self.inner.len()
    }

    /// Checks if the tree is empty.
    /// Returns `true` if the tree contains no elements.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T: Copy + Ord> IntervalsTree<T> {
    /// Returns the biggest point in tree.
    pub fn end(&self) -> Option<T> {
        self.inner.last_key_value().map(|(_, &e)| e)
    }
    /// Returns the smallest point in tree.
    pub fn start(&self) -> Option<T> {
        self.inner.first_key_value().map(|(&s, _)| s)
    }
}

impl<T: Copy> Default for IntervalsTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Debug + Numerated> Debug for IntervalsTree<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: Numerated> IntervalsTree<T> {
    fn into_start_end<I: Into<IntervalIterator<T>>>(interval: I) -> Option<(T, T)> {
        Into::<IntervalIterator<T>>::into(interval)
            .inner()
            .map(|i| i.into_parts())
    }

    #[track_caller]
    fn put(&mut self, start: T, end: T) {
        debug_assert!(start <= end, "Must be guarantied");
        self.inner.insert(start, end);
    }

    /// Returns iterator over all intervals in tree.
    pub fn iter(&self) -> impl Iterator<Item = Interval<T>> + '_ {
        self.inner.iter().map(|(&start, &end)| unsafe {
            // Safe, because `Self` guaranties, that inner contains only `start` ≤ `end`.
            Interval::<T>::new_unchecked(start, end)
        })
    }

    /// Returns true if for each `p` ∈ `interval` ⇒ `p` ∈ `self`, otherwise returns false.
    pub fn contains<I: Into<IntervalIterator<T>>>(&self, interval: I) -> bool {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval is always contained.
            return true;
        };
        self.inner
            .range(..=end)
            .next_back()
            .map(|(&s, &e)| s <= start && end <= e)
            .unwrap_or(false)
    }

    /// Insert interval into tree.
    /// - if `interval` is empty, then nothing will be inserted.
    /// - if `interval` is not empty, then after insertion: for each `p` ∈ `interval` ⇒ `p` ∈ `self`.
    ///
    /// Complexity: `O(m * log(n))`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    ///
    /// Returns whether `self` has been changed.
    pub fn insert<I: Into<IntervalIterator<T>>>(&mut self, interval: I) -> bool {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval - nothing to insert.
            return false;
        };

        let Some(last) = self.end() else {
            // No other intervals, so can just insert as is.
            self.put(start, end);
            return true;
        };

        // If `end` < `last`, then we must take in account next point after `end`,
        // because there can be neighbor interval which must be merged with `interval`.
        let iter_end = end.inc_if_lt(last).unwrap_or(end);
        let mut iter = self.inner.range(..=iter_end).map(|(&s, &e)| (s, e));

        // "right interval" is the interval in `self` with the largest start point
        // that is less than or equal to `iter_end`. This interval is the closest
        // one that may either overlap with or lie immediately adjacent to the
        // interval being inserted.
        let Some((right_start, right_end)) = iter.next_back() else {
            // No neighbor or intersected intervals, so can just insert as is.
            self.put(start, end);
            return true;
        };

        if let Some(right_end) = right_end.inc_if_lt(start) {
            // `right_end` <= `start`, so "right interval" lies before `interval`
            if right_end == start {
                // "right interval" intersects with `interval` in one point: `start`, so join intervals
                self.put(right_start, end);
            } else {
                // no intersections, so insert as is
                self.put(start, end);
            }
            return true;
        } else if right_start <= start {
            if right_end < end {
                // "right interval" starts outside and ends inside `inside`, so can just expand it
                self.put(right_start, end);
                return true;
            } else {
                // nothing to do: our interval is completely inside "right interval".
                return false;
            }
        }

        // `left_interval` is an interval in `self`, which has biggest start,
        // but start must lies before or equal to `interval` start point.
        let mut left_interval = None;
        let mut intervals_to_remove = Vec::new();
        while let Some((s, e)) = iter.next_back() {
            if s <= start {
                left_interval = Some((s, e));
                break;
            }
            intervals_to_remove.push(s);
        }

        // All intervals between `left_interval` and "right interval" will be
        // removed, because they lies completely inside `interval`.
        for start in intervals_to_remove {
            self.inner.remove(&start);
        }

        // In this point `start` < `right_start` ≤ `end`, so in any cases it will be removed.
        self.inner.remove(&right_start);

        let end = right_end.max(end);

        let Some((left_start, left_end)) = left_interval else {
            // no `left_interval` => `interval` has no more intersections and can be inserted now
            self.put(start, end);
            return true;
        };

        debug_assert!(left_end < right_start && left_start <= start);
        let Some(left_end) = left_end.inc_if_lt(right_start) else {
            // Must be `left_end` < `right_start`
            debug_assert!(false, "`T: Numerated` impl error");
            return false;
        };

        if left_end >= start {
            // `left_end` is inside `interval`, so expand `left_interval`
            self.put(left_start, end);
        } else {
            // `left_interval` is outside, so just insert `interval`
            self.put(start, end);
        }

        // Returns true because the current interval has unique points compared to at least `right_interval`
        true
    }

    /// Remove `interval` from tree.
    /// - if `interval` is empty, then nothing will be removed.
    /// - if `interval` is not empty, then after removing: for each `p` ∈ `interval` ⇒ `p` ∉ `self`.
    ///
    /// Complexity: `O(m * log(n))`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    ///
    /// Returns whether `self` has been changed.
    pub fn remove<I: Into<IntervalIterator<T>>>(&mut self, interval: I) -> bool {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval - nothing to remove.
            return false;
        };

        // `iter` iterates over all intervals, which starts before or inside `interval`.
        let mut iter = self.inner.range(..=end);

        // "right interval" - interval from `iter` with the biggest start.
        let Some((&right_start, &right_end)) = iter.next_back() else {
            return false;
        };

        if right_end < start {
            // No intersections with `interval`.
            return false;
        }

        // `left_interval` - interval from `iter` which lies before `interval`
        // and has intersection with `interval`.
        let mut left_interval = None;
        let mut intervals_to_remove = Vec::new();
        while let Some((&s, &e)) = iter.next_back() {
            if s < start {
                if e >= start {
                    left_interval = Some(s);
                }
                break;
            }

            intervals_to_remove.push(s)
        }

        // `intervals_to_remove` contains all intervals,
        // which lies completely inside `interval`, so must be removed.
        for start in intervals_to_remove {
            self.inner.remove(&start);
        }

        if let Some(start) = start.dec_if_gt(right_start) {
            // "right interval" starts before `interval` and has intersection,
            // so "right interval" must be chopped.
            self.put(right_start, start);
        } else {
            // "right interval" is partially/completely inside `interval`,
            // so we remove it here and then put it back with new start if needed.
            debug_assert!(
                right_start <= end,
                "Must be, because of method it was found"
            );
            self.inner.remove(&right_start);
        }

        if let Some(end) = end.inc_if_lt(right_end) {
            // "right interval" ends after `interval`,
            // so after chopping or removing we put the remainder here.
            self.put(end, right_end);
        } else {
            debug_assert!(start <= right_end);
        }

        if let Some(left_start) = left_interval {
            // `left_interval` lies before `interval` and has intersection with `interval`,
            // so it must be chopped.
            if let Some(start) = start.dec_if_gt(left_start) {
                self.put(left_start, start);
            } else {
                debug_assert!(false, "`T: Numerated` impl error");
            }
        }

        // Returns true because the interval intersects with the given range (i.e., right_end >= start),
        // and the interval will be removed or modified accordingly.
        true
    }

    /// Returns iterator over non empty intervals, that consist of points `p: T`
    /// where each `p` ∉ `self` and `p` ∈ `interval`.
    /// Intervals in iterator are sorted in ascending order.
    ///
    /// Iterating complexity: `O(log(n) + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn voids<I: Into<IntervalIterator<T>>>(
        &self,
        interval: I,
    ) -> VoidsIterator<T, impl Iterator<Item = Interval<T>> + '_> {
        let Some((mut start, end)) = Self::into_start_end(interval) else {
            // Empty interval.
            return VoidsIterator { inner: None };
        };

        if let Some((_, &e)) = self.inner.range(..=start).next_back() {
            if let Some(e) = e.inc_if_lt(end) {
                if e > start {
                    start = e;
                }
            } else {
                // `interval` is inside of one of `self` interval - no voids.
                return VoidsIterator { inner: None };
            }
        }

        let iter = self.inner.range(start..=end).map(|(&start, &end)| {
            // Safe, because `Self` guaranties, that inner contains only `start` ≤ `end`.
            unsafe { Interval::new_unchecked(start, end) }
        });

        // Safe, because we have already checked, that `start` ≤ `end`.
        let interval = unsafe { Interval::new_unchecked(start, end) };

        VoidsIterator {
            inner: Some((iter, interval)),
        }
    }

    /// Returns iterator over intervals, which consist of points `p: T`,
    /// where each `p` ∈ `self` and `p` ∉ `other`.
    ///
    /// Iterating complexity: `O(n + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `other`
    pub fn difference<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Interval<T>> + 'a {
        DifferenceIterator {
            iter1: self.iter(),
            iter2: other.iter(),
            interval1: None,
            interval2: None,
        }
    }

    /// Number of points in tree set.
    ///
    /// Complexity: `O(n)`, where `n` is amount of intervals in `self`.
    pub fn points_amount(&self) -> Option<T::Distance> {
        let mut res = T::Distance::zero();
        for interval in self.iter() {
            res = res.checked_add(&interval.raw_len()?)?;
        }
        Some(res)
    }

    /// Iterator over all points in tree set.
    pub fn points_iter(&self) -> impl Iterator<Item = T> + '_ {
        self.inner.iter().flat_map(|(&s, &e)| unsafe {
            // Safe, because `Self` guaranties, that it contains only `start` ≤ `end`
            Interval::new_unchecked(s, e).iter()
        })
    }

    /// Convert tree to vector of inclusive ranges.
    pub fn to_vec(&self) -> Vec<RangeInclusive<T>> {
        self.iter().map(Into::into).collect()
    }
}

impl<T: Numerated, D: Into<IntervalIterator<T>>> FromIterator<D> for IntervalsTree<T> {
    fn from_iter<I: IntoIterator<Item = D>>(iter: I) -> Self {
        let mut tree = Self::new();
        for interval in iter {
            tree.insert(interval);
        }
        tree
    }
}

#[allow(clippy::reversed_empty_ranges)]
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn insert() {
        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(1..=2).unwrap()));
        assert_eq!(tree.to_vec(), vec![1..=2]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(-1..=2).unwrap()));
        assert!(tree.insert(Interval::try_from(4..=5).unwrap()));
        assert_eq!(tree.to_vec(), vec![-1..=2, 4..=5]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(-1..=2).unwrap()));
        assert!(tree.insert(Interval::try_from(3..=4).unwrap()));
        assert_eq!(tree.to_vec(), vec![-1..=4]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(1));
        assert!(tree.insert(2));
        assert_eq!(tree.to_vec(), vec![1..=2]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(-1..=3).unwrap()));
        assert!(tree.insert(Interval::try_from(5..=7).unwrap()));
        assert!(tree.insert(Interval::try_from(2..=6).unwrap()));
        assert!(
            !tree.insert(Interval::try_from(7..=7).unwrap()),
            "Expected false, because point 7 already in tree"
        );
        assert!(tree.insert(Interval::try_from(19..=25).unwrap()));
        assert_eq!(tree.to_vec(), vec![-1..=7, 19..=25]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(-1..=3).unwrap()));
        assert!(tree.insert(Interval::try_from(10..=14).unwrap()));
        assert!(tree.insert(Interval::try_from(4..=9).unwrap()));
        assert_eq!(tree.to_vec(), vec![-1..=14]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(-111..=3).unwrap()));
        assert!(tree.insert(Interval::try_from(10..=14).unwrap()));
        assert!(tree.insert(Interval::try_from(3..=10).unwrap()));
        assert_eq!(tree.to_vec(), vec![-111..=14]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(..=10));
        assert!(
            !tree.insert(Interval::try_from(3..=4).unwrap()),
            "Expected false, because no unique points to insert in 3..=4"
        );
        assert_eq!(tree.to_vec(), vec![i32::MIN..=10]);

        let mut tree = IntervalsTree::new();
        assert!(tree.insert(Interval::try_from(1..=10).unwrap()));
        assert!(
            !tree.insert(Interval::try_from(3..=4).unwrap()),
            "Expected false, because non-empty interval has no unique points to insert in 3..=4"
        );
        assert!(
            !tree.insert(Interval::try_from(5..=6).unwrap()),
            "Expected false, because non-empty interval has no unique points to insert in 5..=6"
        );
        assert_eq!(tree.to_vec(), vec![1..=10]);

        let mut tree = IntervalsTree::new();
        assert_eq!(tree.to_vec(), vec![]);
        assert!(tree.insert(0..));
        assert_eq!(tree.to_vec(), vec![0..=u32::MAX]);

        let mut tree = IntervalsTree::<i32>::new();
        assert!(
            !tree.insert(IntervalIterator::empty()),
            "Expected false, because empty interval don't change self"
        );
    }

    #[test]
    fn remove() {
        let mut tree: IntervalsTree<i32> = [1].into_iter().collect();
        assert!(tree.remove(1));
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree: IntervalsTree<i32> = [1, 2].into_iter().collect();
        assert!(tree.remove(Interval::try_from(1..=2).unwrap()));
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree: IntervalsTree<i32> = [-1, 0, 1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(-1..=2).unwrap()));
        assert_eq!(tree.to_vec(), vec![4..=5]);

        let mut tree: IntervalsTree<i32> = [-1, 0, 1, 2, 4, 5].into_iter().collect();
        tree.remove(Interval::try_from(4..=5).unwrap());
        assert_eq!(tree.to_vec(), vec![-1..=2]);

        let mut tree: IntervalsTree<i32> = [1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(2..=4).unwrap()));
        assert_eq!(tree.to_vec(), vec![1..=1, 5..=5]);

        let mut tree: IntervalsTree<i32> = [-1, 0, 1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(3..=4).unwrap()));
        assert_eq!(tree.to_vec(), vec![-1..=2, 5..=5]);

        let mut tree: IntervalsTree<i32> = [-1, 0, 1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(-1..=5).unwrap()));
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree: IntervalsTree<i32> = [1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(2..=5).unwrap()));
        assert_eq!(tree.to_vec(), vec![1..=1]);

        let mut tree: IntervalsTree<i32> = [1, 2, 4, 5].into_iter().collect();
        assert!(tree.remove(Interval::try_from(1..=4).unwrap()));
        assert_eq!(tree.to_vec(), vec![5..=5]);

        let mut tree: IntervalsTree<i32> = [1, 2, 4, 5].into_iter().collect();
        assert!(
            !tree.remove(Interval::try_from(3..=3).unwrap()),
            "Expected false, because there is no point 3 in tree"
        );
        assert!(tree.remove(Interval::try_from(1..=3).unwrap()));
        assert_eq!(tree.to_vec(), vec![4..=5]);
        assert_eq!(tree.to_vec(), vec![4..=5]);

        let mut tree: IntervalsTree<u32> = [1, 2, 5, 6, 7, 9, 10, 11].into_iter().collect();
        assert_eq!(tree.to_vec(), vec![1..=2, 5..=7, 9..=11]);
        assert!(tree.remove(Interval::try_from(1..2).unwrap()));
        assert_eq!(tree.to_vec(), vec![2..=2, 5..=7, 9..=11]);
        assert!(tree.remove(..7));
        assert_eq!(tree.to_vec(), vec![7..=7, 9..=11]);
        assert!(tree.remove(..));
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree = IntervalsTree::<i32>::new();
        assert!(
            !tree.remove(IntervalIterator::empty()),
            "Expected false, because empty interval don't change self"
        );
    }

    #[test]
    fn voids() {
        let tree: IntervalsTree<u32> = [1..=7, 19..=25]
            .into_iter()
            .map(|i| Interval::try_from(i).unwrap())
            .collect();

        assert_eq!(
            tree.voids(Interval::try_from(0..100).unwrap())
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![0..=0, 8..=18, 26..=99],
        );
        assert_eq!(
            tree.voids(..).map(RangeInclusive::from).collect::<Vec<_>>(),
            vec![0..=0, 8..=18, 26..=u32::MAX],
        );
        assert_eq!(
            tree.voids(IntervalIterator::empty()).collect::<Vec<_>>(),
            Vec::<RangeInclusive<_>>::new()
        );
        assert_eq!(
            tree.voids(0).map(RangeInclusive::from).collect::<Vec<_>>(),
            vec![0..=0],
        );
    }

    #[test]
    fn contains() {
        let tree: IntervalsTree<u64> = [0, 100, 101, 102, 45678, 45679, 1, 2, 3]
            .into_iter()
            .collect();
        assert_eq!(tree.to_vec(), vec![0..=3, 100..=102, 45678..=45679]);
        assert!(tree.contains(0));
        assert!(!tree.contains(4));
        assert!(tree.contains(100));
        assert!(!tree.contains(103));
        assert!(tree.contains(45678));
        assert!(!tree.contains(45680));
        assert!(tree.contains(Interval::try_from(0..=3).unwrap()));
        assert!(!tree.contains(Interval::try_from(0..5).unwrap()));
        assert!(tree.contains(IntervalIterator::empty()));
        assert!(!tree.contains(..));
        assert!(tree.contains(..1));
    }

    #[test]
    fn amount() {
        let tree: IntervalsTree<i32> = [-100, -99, 100, 101, 102, 1000].into_iter().collect();
        assert_eq!(tree.intervals_amount(), 3);
        assert_eq!(tree.points_amount(), Some(6));

        let tree: IntervalsTree<i32> = [..].into_iter().collect();
        assert_eq!(tree.intervals_amount(), 1);
        assert_eq!(tree.points_amount(), None);

        let tree: IntervalsTree<i32> = Default::default();
        assert_eq!(tree.intervals_amount(), 0);
        assert_eq!(tree.points_amount(), Some(0));
    }

    #[test]
    fn start_end() {
        let tree: IntervalsTree<u64> = [0u64, 100, 101, 102, 45678, 45679, 1, 2, 3]
            .into_iter()
            .collect();
        assert_eq!(tree.to_vec(), vec![0..=3, 100..=102, 45678..=45679]);
        assert_eq!(tree.start(), Some(0));
        assert_eq!(tree.end(), Some(45679));
    }

    #[test]
    fn difference() {
        let tree: IntervalsTree<u64> = [0, 1, 2, 3, 4, 8, 9, 100, 101, 102].into_iter().collect();
        let tree1: IntervalsTree<u64> = [3, 4, 7, 8, 9, 10, 45, 46, 100, 102].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.difference(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=2, 101..=101]);

        let tree1: IntervalsTree<u64> = [..].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.difference(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![]);

        let tree1: IntervalsTree<u64> = [..=100].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.difference(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![101..=102]);

        let tree1: IntervalsTree<u64> = [101..].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.difference(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=4, 8..=9, 100..=100]);

        let tree1: IntervalsTree<u64> = [6, 10, 110].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.difference(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=4, 8..=9, 100..=102]);
    }
}
