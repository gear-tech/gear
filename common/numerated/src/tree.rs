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

//! [IntervalsTree] implementation.

use crate::{Interval, NonEmptyInterval, Numerated};
use alloc::{collections::BTreeMap, fmt, fmt::Debug, vec::Vec};
use core::{fmt::Formatter, ops::RangeInclusive};
use num_traits::{CheckedAdd, Zero};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// # Non overlapping intervals tree
/// Can be considered as set of points, but with possibility to work with
/// continuous sets of this points (the same as interval) as fast as with points.
/// Insert and remove operations has complexity between `[O(log(n)), O(n)]`,
/// where `n` is amount of intervals in tree.
/// So, even if you insert for example points from [`0u64`] to [`u64::MAX`],
/// then removing all of them or any part of them is as fast as removing one point.
///
/// # Examples
/// ```
/// use numerated::IntervalsTree;
/// use std::collections::BTreeSet;
/// use std::ops::RangeInclusive;
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
/// // We can add points from 3 to 100_000 only by one insert operation.
/// // `try` is only for range check, that it has start ≤ end.
/// tree.try_insert(3..=100_000).unwrap();
/// // now `tree` contains two intervals: [1..=1] and [3..=100_000]
/// set.extend(3..=100_000);
/// // extend complexity is O(n), where n is amount of elements in range.
/// assert_eq!(set, tree.points_iter().collect());
///
/// // We can remove points from 1 to 99_000 not inclusive only by one remove operation.
/// // `try` is only for range check, that it has start ≤ end.
/// tree.try_remove(1..99_000).unwrap();
/// // now `tree` contains two intervals: [99_000..=100_000]
/// (1..99_000).for_each(|i| { set.remove(&i); });
/// // remove complexity for set is O(n*log(m)),
/// // where `n` is amount of elements in range and `m` size of `tree`.
/// assert_eq!(set, tree.points_iter().collect());
///
/// // Can insert or remove all possible points just by one operation:
/// tree.insert(..);
/// tree.remove(..);
///
/// // Iterate over voids (intervals between intervals in tree):
/// tree.try_insert(1..=3).unwrap();
/// tree.try_insert(5..=7).unwrap();
/// let voids = tree.voids(..).map(RangeInclusive::from).collect::<Vec<_>>();
/// assert_eq!(voids, vec![i32::MIN..=0, 4..=4, 8..=i32::MAX]);
///
/// // AndNot iterator: iterate over intervals from `tree` which are not in `other_tree`.
/// let other_tree: IntervalsTree<i32> = [3, 4, 5, 7, 8, 9].iter().collect();
/// let and_not: Vec<_> = tree.and_not_iter(&other_tree).map(RangeInclusive::from).collect();
/// assert_eq!(and_not, vec![1..=2, 6..=6]);
/// ```
///
/// # Possible panic cases
/// Using `IntervalsTree` for type `T: Numerated` cannot cause panics,
/// if implementation [Numerated], [Copy], [Ord], [Eq] are correct for `T`.
/// In other cases `IntervalsTree` does not guarantees execution without panics.
#[derive(Clone, PartialEq, Eq, TypeInfo, Encode, Decode)]
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

impl<T: Debug> Debug for IntervalsTree<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}",
            self.inner.iter().map(|(s, e)| s..=e).collect::<Vec<_>>()
        )
    }
}

impl<T: Numerated> IntervalsTree<T> {
    fn into_start_end<I: Into<Interval<T>>>(interval: I) -> Option<(T, T)> {
        Into::<Interval<T>>::into(interval).into_inner()
    }

    /// Returns iterator over all intervals in tree.
    pub fn iter(&self) -> impl Iterator<Item = NonEmptyInterval<T>> + '_ {
        self.inner.iter().map(|(&start, &end)| unsafe {
            // Safe, because `Self` guaranties, that inner contains only `start` ≤ `end`.
            NonEmptyInterval::<T>::new_unchecked(start, end)
        })
    }

    /// Returns true if for each `p` ∈ `interval` ⇒ `p` ∈ `self`, otherwise returns false.
    pub fn contains<I: Into<Interval<T>>>(&self, interval: I) -> bool {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval is always contained.
            return true;
        };
        if let Some((&s, &e)) = self.inner.range(..=end).next_back() {
            if s <= start {
                return e >= end;
            }
        }
        false
    }

    /// The same as [`Self::contains`], but returns [`I::Error`] if `try_into` [Interval] fails.
    pub fn try_contains<I: TryInto<Interval<T>>>(&self, interval: I) -> Result<bool, I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        Ok(self.contains(interval))
    }

    /// Insert interval into tree.
    /// - if `interval` is empty, then nothing will be inserted.
    /// - if `interval` is not empty, then after inserting: for each `p` ∈ `interval` ⇒ `p` ∈ `self`.
    ///
    /// Complexity: `O(log(n) + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn insert<I: Into<Interval<T>>>(&mut self, interval: I) {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval - nothing to insert.
            return;
        };

        let Some(last) = self.end() else {
            // No other intervals, so can just insert as is.
            self.inner.insert(start, end);
            return;
        };

        let mut iter = if let Some(point_after_end) = end.inc_if_lt(last) {
            // If `end` < `last`, then we must take in account next point after `end`,
            // because there can be neighbor interval which must be merged with `interval`.
            self.inner.range(..=point_after_end)
        } else {
            self.inner.range(..=end)
        }
        .map(|(&s, &e)| (s, e));

        let Some((right_start, right_end)) = iter.next_back() else {
            // No neighbor or intersected intervals, so can just insert as is.
            self.inner.insert(start, end);
            return;
        };

        if let Some(right_end) = right_end.inc_if_lt(start) {
            if right_end == start {
                self.inner.insert(right_start, end);
            } else {
                self.inner.insert(start, end);
            }
            return;
        } else if right_start <= start {
            if right_end < end {
                self.inner.insert(right_start, end);
            } else {
                // nothing to do: our interval is completely inside "right interval".
            }
            return;
        }

        let mut left_interval = None;
        let mut intervals_to_remove = Vec::new();
        while let Some((s, e)) = iter.next_back() {
            if s <= start {
                left_interval = Some((s, e));
                break;
            }
            intervals_to_remove.push(s);
        }
        for start in intervals_to_remove {
            self.inner.remove(&start);
        }

        // In this point `start` < `right_start` ≤ `end`, so in any cases it will be removed.
        self.inner.remove(&right_start);

        let end = right_end.max(end);

        let Some((left_start, left_end)) = left_interval else {
            self.inner.insert(start, end);
            return;
        };

        debug_assert!(left_end < right_start);
        debug_assert!(left_start <= start);
        let Some(left_end) = left_end.inc_if_lt(right_start) else {
            unreachable!(
                "`T: Numerated` impl error: for each x: T, y: T, x < y ↔ x.inc_if_lt(y) == Some(_)"
            );
        };

        if left_end >= start {
            self.inner.insert(left_start, end);
        } else {
            self.inner.insert(start, end);
        }
    }

    /// The same as [`Self::insert`], but returns [`I::Error`] if `try_into` [Interval] fails.
    pub fn try_insert<I: TryInto<Interval<T>>>(&mut self, interval: I) -> Result<(), I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        self.insert(interval);
        Ok(())
    }

    /// Remove `interval` from tree.
    /// - if `interval` is empty, then nothing will be removed.
    /// - if `interval` is not empty, then after removing for each `p` ∈ `interval` ⇒ `p` ∉ `self`.
    ///
    /// Complexity: `O(log(n) + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn remove<I: Into<Interval<T>>>(&mut self, interval: I) {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval - nothing to remove.
            return;
        };

        let mut iter = self.inner.range(..=end);

        let Some((&right_start, &right_end)) = iter.next_back() else {
            return;
        };

        if right_end < start {
            return;
        }

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

        for start in intervals_to_remove {
            self.inner.remove(&start);
        }

        if let Some(start) = start.dec_if_gt(right_start) {
            debug_assert!(start >= right_start);
            self.inner.insert(right_start, start);
        } else {
            debug_assert!(right_start <= end);
            self.inner.remove(&right_start);
        }

        if let Some(end) = end.inc_if_lt(right_end) {
            debug_assert!(end <= right_end);
            self.inner.insert(end, right_end);
        } else {
            debug_assert!(start <= right_end);
        }

        if let Some(left_start) = left_interval {
            // `left_start` < `start` cause of method it was found.
            debug_assert!(left_start < start);
            if let Some(start) = start.dec_if_gt(left_start) {
                self.inner.insert(left_start, start);
            } else {
                unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y ⇔ x.dec_if_gt(y) == Some(_)");
            }
        }
    }

    /// The same as [`Self::remove`], but returns [`I::Error`] if `try_into` [Interval] fails.
    pub fn try_remove<I: TryInto<Interval<T>>>(&mut self, interval: I) -> Result<(), I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        self.remove(interval);
        Ok(())
    }

    /// Returns iterator over non empty intervals, that consist of points `p: T`
    /// where each `p` ∉ `self` and `p` ∈ `interval`.
    /// Intervals in iterator are sorted in ascending order.
    ///
    /// Iterating complexity: `O(log(n) + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn voids<I: Into<Interval<T>>>(
        &self,
        interval: I,
    ) -> VoidsIterator<T, impl Iterator<Item = NonEmptyInterval<T>> + '_> {
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
            unsafe { NonEmptyInterval::new_unchecked(start, end) }
        });

        // Safe, because we have already checked, that `start` ≤ `end`.
        let interval = unsafe { NonEmptyInterval::new_unchecked(start, end) };

        VoidsIterator {
            inner: Some((iter, interval)),
        }
    }

    /// The same as [`Self::voids`], but returns [`I::Error`] if `try_into` [Interval] fails.
    pub fn try_voids<I: TryInto<Interval<T>>>(
        &self,
        interval: I,
    ) -> Result<VoidsIterator<T, impl Iterator<Item = NonEmptyInterval<T>> + '_>, I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        Ok(self.voids(interval))
    }

    /// Returns iterator over intervals which consist of points `p: T`,
    /// where each `p` ∈ `self` and `p` ∉ `other`.
    ///
    /// Iterating complexity: `O(n + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `other`
    pub fn and_not_iter<'a: 'b, 'b: 'a>(
        &'a self,
        other: &'b Self,
    ) -> impl Iterator<Item = NonEmptyInterval<T>> + '_ {
        AndNotIterator {
            iter1: self.iter(),
            iter2: other.iter(),
            interval1: None,
            interval2: None,
        }
    }

    /// Number of points in tree set.
    ///
    /// Complexity: `O(n)`, where `n` is amount of intervals in `self`.
    pub fn points_amount(&self) -> Option<T::N> {
        let mut res = T::N::zero();
        for interval in self.iter() {
            res = res.checked_add(&interval.raw_size()?)?;
        }
        Some(res)
    }

    /// Iterator over all points in tree set.
    pub fn points_iter(&self) -> impl Iterator<Item = T> + '_ {
        self.inner.iter().flat_map(|(&s, &e)| unsafe {
            // Safe, because `Self` guaranties, that it contains only `start` ≤ `end`
            NonEmptyInterval::new_unchecked(s, e).iter()
        })
    }

    /// Convert tree to vector of inclusive ranges.
    pub fn to_vec(&self) -> Vec<RangeInclusive<T>> {
        self.iter().map(Into::into).collect()
    }
}

/// Helper struct to iterate over voids in tree.
///
/// See also [`IntervalsTree::voids`].
pub struct VoidsIterator<T: Numerated, I: Iterator<Item = NonEmptyInterval<T>>> {
    inner: Option<(I, NonEmptyInterval<T>)>,
}

impl<T: Numerated, I: Iterator<Item = NonEmptyInterval<T>>> Iterator for VoidsIterator<T, I> {
    type Item = NonEmptyInterval<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (iter, interval) = self.inner.as_mut()?;
        if let Some(next) = iter.next() {
            let (start, end) = next.into_inner();

            // Guaranties by tree: between two intervals always exists void.
            debug_assert!(interval.start() < start);

            let void_end = start.dec_if_gt(interval.start()).unwrap_or_else(|| {
                unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y ↔ x.dec_if_gt(y) == Some(_)");
            });

            let res = NonEmptyInterval::new(interval.start(), void_end);
            if res.is_none() {
                unreachable!(
                    "`T: Numerated` impl error: for each x: T, y: T, x > y ↔ x.dec_if_gt(y) ≥ y"
                );
            }

            if let Some(new_start) = end.inc_if_lt(interval.end()) {
                *interval = NonEmptyInterval::new(new_start, interval.end()).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each x: T, y: T, x < y ↔ x.inc_if_lt(y) ≤ y");
                });
            } else {
                self.inner = None;
            }

            res
        } else {
            let res = Some(*interval);
            self.inner = None;
            res
        }
    }
}

/// Helper struct to iterate over intervals from `tree`, which are not in `other_tree`.
///
/// See also [`IntervalsTree::and_not_iter`].
pub struct AndNotIterator<
    T: Numerated,
    I1: Iterator<Item = NonEmptyInterval<T>>,
    I2: Iterator<Item = NonEmptyInterval<T>>,
> {
    iter1: I1,
    iter2: I2,
    interval1: Option<NonEmptyInterval<T>>,
    interval2: Option<NonEmptyInterval<T>>,
}

impl<
        T: Numerated,
        I1: Iterator<Item = NonEmptyInterval<T>>,
        I2: Iterator<Item = NonEmptyInterval<T>>,
    > Iterator for AndNotIterator<T, I1, I2>
{
    type Item = NonEmptyInterval<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let interval1 = if let Some(interval1) = self.interval1 {
                interval1
            } else {
                let interval1 = self.iter1.next()?;
                self.interval1 = Some(interval1);
                interval1
            };

            let interval2 = if let Some(interval2) = self.interval2 {
                interval2
            } else if let Some(interval2) = self.iter2.next() {
                interval2
            } else {
                self.interval1 = None;
                return Some(interval1);
            };

            if interval2.end() < interval1.start() {
                self.interval2 = None;
                continue;
            }

            self.interval2 = Some(interval2);

            if interval1.end() < interval2.start() {
                self.interval1 = None;
                return Some(interval1);
            } else {
                if let Some(new_start) = interval2.end().inc_if_lt(interval1.end()) {
                    self.interval1 = NonEmptyInterval::new(new_start, interval1.end());
                    if self.interval1.is_none() {
                        unreachable!("`T: Numerated` impl error: for each x: T, y: T, x < y ⇔ x.inc_if_lt(y) ≤ y");
                    }
                } else if interval1.end() == interval2.end() {
                    self.interval1 = None;
                    self.interval2 = None;
                } else {
                    self.interval1 = None;
                }

                if let Some(new_end) = interval2.start().dec_if_gt(interval1.start()) {
                    let res = NonEmptyInterval::new(interval1.start(), new_end);
                    if res.is_none() {
                        unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y ⇔ x.dec_if_gt(y) ≥ y");
                    }
                    return res;
                } else {
                    continue;
                }
            }
        }
    }
}

impl<T: Numerated, D: Into<Interval<T>>> FromIterator<D> for IntervalsTree<T> {
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
        tree.try_insert(1..=2).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=2]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=2, 4..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(3..=4).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=4]);

        let mut tree = IntervalsTree::new();
        tree.insert(1);
        tree.insert(2);
        assert_eq!(tree.to_vec(), vec![1..=2]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=3).unwrap();
        tree.try_insert(5..=7).unwrap();
        tree.try_insert(2..=6).unwrap();
        tree.try_insert(7..=7).unwrap();
        tree.try_insert(19..=25).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=7, 19..=25]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=3).unwrap();
        tree.try_insert(10..=14).unwrap();
        tree.try_insert(4..=9).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=14]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-111..=3).unwrap();
        tree.try_insert(10..=14).unwrap();
        tree.try_insert(3..=10).unwrap();
        assert_eq!(tree.to_vec(), vec![-111..=14]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(i32::MIN..=10).unwrap();
        tree.try_insert(3..=4).unwrap();
        assert_eq!(tree.to_vec(), vec![i32::MIN..=10]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=10).unwrap();
        tree.try_insert(3..=4).unwrap();
        tree.try_insert(5..=6).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=10]);
    }

    #[test]
    fn remove() {
        let mut tree = IntervalsTree::new();
        tree.insert(1);
        tree.remove(1);
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=2).unwrap();
        tree.try_remove(1..=2).unwrap();
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(-1..=2).unwrap();
        assert_eq!(tree.to_vec(), vec![4..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(4..=5).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=2]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(2..=4).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=1, 5..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(3..=4).unwrap();
        assert_eq!(tree.to_vec(), vec![-1..=2, 5..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(-1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(-1..=5).unwrap();
        assert_eq!(tree.to_vec(), vec![]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(2..=5).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=1]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(1..=4).unwrap();
        assert_eq!(tree.to_vec(), vec![5..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=2).unwrap();
        tree.try_insert(4..=5).unwrap();
        tree.try_remove(1..=3).unwrap();
        assert_eq!(tree.to_vec(), vec![4..=5]);

        let mut tree = IntervalsTree::new();
        tree.try_insert(1..=10).unwrap();
        assert_eq!(tree.clone().to_vec(), vec![1..=10]);
        tree.remove(10..);
        assert_eq!(tree.clone().to_vec(), vec![1..=9]);
        tree.remove(2..);
        assert_eq!(tree.clone().to_vec(), vec![1..=1]);
        tree.try_insert(3..6).unwrap();
        assert_eq!(tree.clone().to_vec(), vec![1..=1, 3..=5]);
        tree.remove(..=3);
        assert_eq!(tree.clone().to_vec(), vec![4..=5]);
        tree.try_insert(1..=2).unwrap();
        assert_eq!(tree.clone().to_vec(), vec![1..=2, 4..=5]);
        tree.remove(..);
        assert_eq!(tree.clone().to_vec(), vec![]);
        tree.insert(..);
        assert_eq!(tree.clone().to_vec(), vec![i32::MIN..=i32::MAX]);
        tree.remove(..=9);
        assert_eq!(tree.clone().to_vec(), vec![10..=i32::MAX]);
        tree.remove(21..);
        assert_eq!(tree.clone().to_vec(), vec![10..=20]);
    }

    #[test]
    fn try_voids() {
        let mut tree = IntervalsTree::new();
        tree.try_insert(1u32..=7).unwrap();
        tree.try_insert(19..=25).unwrap();
        assert_eq!(tree.clone().to_vec(), vec![1..=7, 19..=25]);
        assert_eq!(
            tree.try_voids(0..100)
                .unwrap()
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![0..=0, 8..=18, 26..=99]
        );
        assert_eq!(
            tree.try_voids((0, None))
                .unwrap()
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![0..=0, 8..=18, 26..=u32::MAX]
        );
        assert_eq!(
            tree.try_voids((None, None))
                .unwrap()
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![]
        );
        assert_eq!(
            tree.try_voids(1..1)
                .unwrap()
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![]
        );
        assert_eq!(
            tree.try_voids(0..=0)
                .unwrap()
                .map(RangeInclusive::from)
                .collect::<Vec<_>>(),
            vec![0..=0]
        );

        assert!(tree.try_voids(1..0).is_err());
    }

    #[test]
    fn try_insert() {
        let mut tree = IntervalsTree::new();
        tree.try_insert(1u32..=2).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=2]);
        tree.try_insert(4..=5).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=2, 4..=5]);
        tree.try_insert(4..4).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=2, 4..=5]);
        assert!(tree.try_insert(4..3).is_err());
        tree.try_insert(None..None).unwrap();
        assert_eq!(tree.to_vec(), vec![1..=2, 4..=5]);
        tree.try_insert((0, None)).unwrap();
        assert_eq!(tree.to_vec(), vec![0..=u32::MAX]);
    }

    #[test]
    fn try_remove() {
        let mut tree = [1u32, 2, 5, 6, 7, 9, 10, 11]
            .into_iter()
            .collect::<IntervalsTree<_>>();
        assert_eq!(tree.to_vec(), vec![1..=2, 5..=7, 9..=11]);
        assert!(tree.try_remove(0..0).is_ok());
        assert_eq!(tree.to_vec(), vec![1..=2, 5..=7, 9..=11]);
        assert!(tree.try_remove(1..1).is_ok());
        assert_eq!(tree.to_vec(), vec![1..=2, 5..=7, 9..=11]);
        assert!(tree.try_remove(1..2).is_ok());
        assert_eq!(tree.to_vec(), vec![2..=2, 5..=7, 9..=11]);
        assert!(tree.try_remove(..7).is_ok());
        assert_eq!(tree.to_vec(), vec![7..=7, 9..=11]);
        assert!(tree.try_remove(None..None).is_ok());
        assert_eq!(tree.to_vec(), vec![7..=7, 9..=11]);
        assert!(tree.try_remove(1..0).is_err());
        assert_eq!(tree.to_vec(), vec![7..=7, 9..=11]);
        assert!(tree.try_remove((1, None)).is_ok());
        assert_eq!(tree.to_vec(), vec![]);
    }

    #[test]
    fn contains() {
        let tree: IntervalsTree<u64> = [0, 100, 101, 102, 45678, 45679, 1, 2, 3]
            .into_iter()
            .collect();
        assert_eq!(tree.to_vec(), vec![0..=3, 100..=102, 45678..=45679]);
        assert!(tree.contains(0));
        assert!(tree.contains(1));
        assert!(tree.contains(2));
        assert!(tree.contains(3));
        assert!(!tree.contains(4));
        assert!(!tree.contains(99));
        assert!(tree.contains(100));
        assert!(tree.contains(101));
        assert!(tree.contains(102));
        assert!(!tree.contains(103));
        assert!(!tree.contains(45677));
        assert!(tree.contains(45678));
        assert!(tree.contains(45679));
        assert!(!tree.contains(45680));
        assert!(!tree.contains(141241));
        assert!(tree.try_contains(0..=3).unwrap());
        assert!(tree.try_contains(0..4).unwrap());
        assert!(!tree.try_contains(0..5).unwrap());
        assert!(tree.try_contains(1..1).unwrap());
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
    fn and_not_iter() {
        let tree: IntervalsTree<u64> = [0, 1, 2, 3, 4, 8, 9, 100, 101, 102].into_iter().collect();
        let tree1: IntervalsTree<u64> = [3, 4, 7, 8, 9, 10, 45, 46, 100, 102].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.and_not_iter(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=2, 101..=101]);

        let tree1: IntervalsTree<u64> = [..].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.and_not_iter(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![]);

        let tree1: IntervalsTree<u64> = [..=100].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.and_not_iter(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![101..=102]);

        let tree1: IntervalsTree<u64> = [101..].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.and_not_iter(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=4, 8..=9, 100..=100]);

        let tree1: IntervalsTree<u64> = [6, 10, 110].into_iter().collect();
        let v: Vec<RangeInclusive<u64>> = tree.and_not_iter(&tree1).map(Into::into).collect();
        assert_eq!(v, vec![0..=4, 8..=9, 100..=102]);
    }
}
