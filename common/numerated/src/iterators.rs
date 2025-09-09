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

//! [`IntervalIterator`], [`VoidsIterator`], [`DifferenceIterator`] implementations.

use crate::{
    interval::{IncorrectRangeError, Interval, NewWithLenError, TryFromRangeError},
    numerated::Numerated,
};
use core::{
    fmt::{self, Debug, Display, Formatter},
    ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive},
};
use num_traits::bounds::{LowerBounded, UpperBounded};

/// Describes interval `start..=end`, which can be empty.
#[derive(Clone, Copy, PartialEq, Eq, Debug, derive_more::From)]
pub struct IntervalIterator<T>(Option<Interval<T>>);

impl<T: Numerated> IntervalIterator<T> {
    /// New empty interval.
    pub fn empty() -> Self {
        Self(None)
    }
    /// Returns whether interval is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    /// Returns inner value.
    pub fn inner(&self) -> Option<Interval<T>> {
        self.0
    }
}

impl<T: Numerated> From<Interval<T>> for IntervalIterator<T> {
    fn from(interval: Interval<T>) -> Self {
        Self(Some(interval))
    }
}

impl<T: Numerated> From<T> for IntervalIterator<T> {
    fn from(point: T) -> Self {
        Interval::from(point).into()
    }
}

impl<T: Numerated + LowerBounded> From<RangeToInclusive<T>> for IntervalIterator<T> {
    fn from(range: RangeToInclusive<T>) -> Self {
        Interval::from(range).into()
    }
}

impl<T: Numerated + UpperBounded, I: Into<T::Bound>> From<RangeFrom<I>> for IntervalIterator<T> {
    fn from(range: RangeFrom<I>) -> Self {
        Interval::try_from(range)
            .map(Into::into)
            .unwrap_or(Self::empty())
    }
}

impl<T: Numerated + LowerBounded + UpperBounded> From<RangeFull> for IntervalIterator<T> {
    fn from(range: RangeFull) -> Self {
        Interval::from(range).into()
    }
}

impl<T: Numerated + LowerBounded + UpperBounded, I: Into<T::Bound>> From<RangeTo<I>>
    for IntervalIterator<T>
{
    fn from(range: RangeTo<I>) -> Self {
        Interval::try_from(range)
            .map(Into::into)
            .unwrap_or(Self::empty())
    }
}

impl<T, S, E> TryFrom<(S, E)> for IntervalIterator<T>
where
    T: Numerated + UpperBounded,
    S: Into<T::Bound>,
    E: Into<T::Bound>,
{
    type Error = IncorrectRangeError;

    fn try_from(range: (S, E)) -> Result<Self, Self::Error> {
        match Interval::try_from(range) {
            Ok(interval) => Ok(interval.into()),
            Err(TryFromRangeError::EmptyRange) => Ok(Self::empty()),
            Err(TryFromRangeError::IncorrectRange) => Err(IncorrectRangeError),
        }
    }
}

impl<T: Numerated + UpperBounded, I: Into<T::Bound>> TryFrom<Range<I>> for IntervalIterator<T> {
    type Error = IncorrectRangeError;

    fn try_from(range: Range<I>) -> Result<Self, Self::Error> {
        Self::try_from((range.start, range.end))
    }
}

impl<T: Numerated> TryFrom<RangeInclusive<T>> for IntervalIterator<T> {
    type Error = IncorrectRangeError;

    fn try_from(range: RangeInclusive<T>) -> Result<Self, Self::Error> {
        Interval::try_from(range).map(Into::into)
    }
}

impl<T: Numerated> Iterator for IntervalIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.map(|interval| {
            let start = interval.start();
            self.0 = interval.inc_start();
            start
        })
    }
}

/// Trying to make interval with end bigger than [`Numerated`] type max value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfBoundsError;

impl<T: Numerated + UpperBounded> IntervalIterator<T> {
    /// Returns interval `start..start + len` if it's possible.
    /// - if `len == None`, then it is supposed, that `len == T::Distance::max_value() + 1`.
    /// - if `start + len - 1` is out of `T`, then returns [`OutOfBoundsError`].
    /// - if `len` is zero, then returns empty interval.
    pub fn with_len<S: Into<T::Bound>, L: Into<Option<T::Distance>>>(
        start: S,
        len: L,
    ) -> Result<Self, OutOfBoundsError> {
        match Interval::with_len(start, len) {
            Ok(interval) => Ok(interval.into()),
            Err(NewWithLenError::ZeroLen) => Ok(Self::empty()),
            Err(NewWithLenError::OutOfBounds) => Err(OutOfBoundsError),
        }
    }
}

impl<T: Display> Display for IntervalIterator<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(interval) = &self.0 {
            write!(f, "{interval}")
        } else {
            write!(f, "∅")
        }
    }
}

/// Helper struct to iterate over intervals from `tree1`, which are not in `tree2`.
///
/// See also [`IntervalsTree::difference`](crate::tree::IntervalsTree::difference).
pub struct DifferenceIterator<T: Numerated, I: Iterator<Item = Interval<T>>> {
    /// Iterator over intervals in `tree1`.
    pub(crate) iter1: I,
    /// Iterator over intervals in `tree2`.
    pub(crate) iter2: I,
    /// Current interval from `tree1`. Starts from `None`.
    pub(crate) interval1: Option<Interval<T>>,
    /// Current interval from `tree2`. Starts from `None`.
    pub(crate) interval2: Option<Interval<T>>,
}

impl<T: Numerated, I: Iterator<Item = Interval<T>>> Iterator for DifferenceIterator<T, I> {
    type Item = Interval<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If `self.interval1` is `None`, then takes next interval from `tree1`.
            // If there isn't any left intervals in `tree1`, then returns `None` - end of iteration.
            self.interval1 = self.interval1.or_else(|| self.iter1.next());
            let interval1 = self.interval1?;

            // If `self.interval2` is `None`, then takes next interval from `tree2`.
            // If there isn't any left intervals in `tree2`, then there is no more intersections
            // and it can return next intervals from `tree1` until the end.
            // Set `self.interval1` to None, to take next interval from `tree1` on next iteration.
            let interval2 = self.interval2.or_else(|| self.iter2.next());
            let Some(interval2) = interval2 else {
                return self.interval1.take();
            };

            // If `interval2` ends before `interval1` starts, then there is no intersection between
            // current `interval2` and `interval1`, so we continue iterate over `tree2`, until
            // the end of `tree2` or until `interval2` starts after `interval1`.
            // Set `self.interval2` to None, to take next interval from `tree2` on next iteration.
            if interval2.end() < interval1.start() {
                self.interval2 = None;
                continue;
            }

            self.interval2 = Some(interval2);

            if interval1.end() < interval2.start() {
                // If `interval1` ends before `interval2` starts, then there is no intersection between
                // current `interval1` and `interval2`, so we can return `interval1`.
                // Set `self.interval1` to None, to take next interval from `tree1` on next iteration.
                self.interval1 = None;
                return Some(interval1);
            } else {
                // In that case `interval1` and `interval2` intersects.
                if let Some(new_start) = interval2.end().inc_if_lt(interval1.end()) {
                    // If `interval2` ends before `interval1`, then we set `self.interval1` to
                    // (interval2.end, interval1.end], so it will be returned on next loop iteration.
                    self.interval1 = Interval::new(new_start, interval1.end());
                    debug_assert!(self.interval1.is_some(), "`T: Numerated` impl error");
                } else if interval1.end() == interval2.end() {
                    // If `interval1` and `interval2` ends at the same point, then
                    // we set both as `None` to take next intervals for both trees
                    // on next loop iteration.
                    self.interval1 = None;
                    self.interval2 = None;
                } else {
                    // If `interval1` ends before `interval2` end,
                    // then set interval1 as `None` to take next interval for `tree1`
                    // on next loop iteration.
                    self.interval1 = None;
                }

                // If `interval1` starts before `interval2`, then we can return
                // [interval1.start, interval2.start) as a result for this iteration.
                // In other case we continue to search for next interval.
                if let Some(new_end) = interval2.start().dec_if_gt(interval1.start()) {
                    let res = Interval::new(interval1.start(), new_end);
                    debug_assert!(res.is_some(), "`T: Numerated` impl error");
                    return res;
                } else {
                    continue;
                }
            }
        }
    }
}

/// Helper struct to iterate over voids in tree.
///
/// See also [`IntervalsTree::voids`](crate::tree::IntervalsTree::voids).
pub struct VoidsIterator<T: Numerated, I: Iterator<Item = Interval<T>>> {
    pub(crate) inner: Option<(I, Interval<T>)>,
}

impl<T: Numerated, I: Iterator<Item = Interval<T>>> Iterator for VoidsIterator<T, I> {
    type Item = Interval<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (iter, interval) = self.inner.as_mut()?;

        // `iter` is an iterator over all intervals in tree.
        // `interval` is an interval where we are searching for voids.
        // It must be guarantied by `Self` creator, that in the beginning
        // `interval.start()` is less than `iter.first().start()`.
        // On each iteration `Self` maintains this invariant.

        // On each iteration we takes next interval from `iter`, let's name it `next`.
        // The resulting void is `[interval.start(), next.start())`.
        // Then we chop `next` from `interval` and continue to search for next voids:
        // `interval = (next.end(), interval.end()]`.
        // If next.end() is bigger or equal to interval.end(), then we set `self.inner` to None,
        // because no other voids can be found.

        if let Some(next) = iter.next() {
            let (start, end) = next.into_parts();

            // Guarantied by [`IntervalsTree`]: between two intervals always exists void.
            debug_assert!(interval.start() < start);

            let Some(void_end) = start.dec_if_gt(interval.start()) else {
                debug_assert!(false, "`T: Numerated` impl error");
                return None;
            };

            let res = Interval::new(interval.start(), void_end);
            debug_assert!(res.is_some(), "`T: Numerated` impl error");

            if let Some(new_start) = end.inc_if_lt(interval.end()) {
                let Some(new_interval) = Interval::new(new_start, interval.end()) else {
                    debug_assert!(false, "`T: Numerated` impl error");
                    return None;
                };
                *interval = new_interval;
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

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn empty_interval() {
        assert!(IntervalIterator::<u8>::empty().is_empty());
        assert!(IntervalIterator::<u8>::with_len(1, 0).unwrap().is_empty());
        assert!(
            IntervalIterator::<u8>::with_len(None, 0)
                .unwrap()
                .is_empty()
        );
        assert!(IntervalIterator::<u8>::try_from(1..1).unwrap().is_empty());
        assert!(IntervalIterator::<u8>::from(..0).is_empty());
        assert!(IntervalIterator::<u8>::from(None..).is_empty());
    }

    #[test]
    fn voids_iterator() {
        let intervals = vec![1u8..3, 5..7]
            .into_iter()
            .map(|r| Interval::<u8>::try_from(r).unwrap());
        let mut iter = VoidsIterator {
            inner: Some((intervals, Interval::<u8>::from(..=10))),
        };
        assert_eq!(Some(0..=0), iter.next().map(Into::into));
        assert_eq!(Some(3..=4), iter.next().map(Into::into));
        assert_eq!(Some(7..=10), iter.next().map(Into::into));
        assert_eq!(None, iter.next());
    }

    #[test]
    fn difference_iterator() {
        let intervals1 = vec![
            Interval::<u8>::try_from(1..3).unwrap(),
            Interval::<u8>::try_from(5..7).unwrap(),
        ];
        let intervals2 = vec![
            Interval::<u8>::try_from(2..4).unwrap(),
            Interval::<u8>::try_from(6..8).unwrap(),
        ];
        let mut iter = DifferenceIterator {
            iter1: intervals1.into_iter(),
            iter2: intervals2.into_iter(),
            interval1: None,
            interval2: None,
        };
        assert_eq!(Some(1..=1), iter.next().map(Into::into));
        assert_eq!(Some(5..=5), iter.next().map(Into::into));
        assert_eq!(None, iter.next());
    }
}
