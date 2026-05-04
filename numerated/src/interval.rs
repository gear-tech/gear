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

//! [`Interval`] implementations.

use crate::{
    iterators::IntervalIterator,
    numerated::{Bound, Numerated},
};
use core::{
    fmt::{self, Debug, Formatter},
    ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive},
};
use num_traits::{
    CheckedAdd, One, Zero,
    bounds::{LowerBounded, UpperBounded},
};

/// Describes not empty interval start..=end.
#[derive(Clone, Copy, PartialEq, Eq, derive_more::Display)]
#[display("{start}..={end}")]
pub struct Interval<T> {
    start: T,
    end: T,
}

impl<T: Numerated> Interval<T> {
    /// Creates new interval `start..=end` with checks only in debug mode.
    ///
    /// # Safety
    ///
    /// Unsafe, because allows to create invalid interval.
    /// Safe, if `start ≤ end`.
    #[track_caller]
    pub unsafe fn new_unchecked(start: T, end: T) -> Self {
        debug_assert!(
            start <= end,
            "Calling this method you must guarantee that `start ≤ end`"
        );
        Self { start, end }
    }

    /// Creates new interval start..=end if start ≤ end, else returns None.
    pub fn new(start: T, end: T) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    /// Interval start (the smallest value inside interval)
    pub fn start(&self) -> T {
        self.start
    }

    /// Interval end (the biggest value inside interval)
    pub fn end(&self) -> T {
        self.end
    }

    /// Converts to [`IntervalIterator`].
    pub fn iter(&self) -> IntervalIterator<T> {
        (*self).into()
    }

    /// Into (start, end)
    pub fn into_parts(self) -> (T, T) {
        self.into()
    }

    /// Returns new [`Interval`] with `start` = `start` + 1, if it's possible.
    pub fn inc_start(&self) -> Option<Self> {
        let (start, end) = (self.start, self.end);
        debug_assert!(start <= end, "It's guaranteed by `Interval`");
        start.inc_if_lt(end).map(|start| {
            debug_assert!(start <= end, "`T: Numerated` impl error");
            Interval { start, end }
        })
    }

    /// Trying to make [`Interval`] from `range`.
    /// - If `range.start > range.end`, then returns [`IncorrectRangeError`].
    /// - If `range.start == range.end`, then returns [`EmptyRangeError`].
    pub fn try_from_range(range: Range<T>) -> Result<Self, TryFromRangeError> {
        let (start, end) = (range.start, range.end);
        end.dec_if_gt(start)
            .map(|end| {
                debug_assert!(start <= end, "`T: Numerated` impl error");
                Self { start, end }
            })
            .ok_or(if start == end {
                TryFromRangeError::EmptyRange
            } else {
                TryFromRangeError::IncorrectRange
            })
    }
}

impl<T: Numerated> PartialEq<RangeInclusive<T>> for Interval<T> {
    fn eq(&self, other: &RangeInclusive<T>) -> bool {
        let (start, end) = self.into_parts();
        (start, end) == (*other.start(), *other.end())
    }
}

impl<T: Numerated> From<Interval<T>> for (T, T) {
    fn from(interval: Interval<T>) -> (T, T) {
        (interval.start, interval.end)
    }
}

impl<T: Numerated> From<Interval<T>> for RangeInclusive<T> {
    fn from(interval: Interval<T>) -> Self {
        interval.start..=interval.end
    }
}

impl<T: Numerated> From<T> for Interval<T> {
    fn from(point: T) -> Self {
        let (start, end) = (point, point);
        debug_assert!(start <= end, "`T: Ord` impl error");
        Self { start, end }
    }
}

impl<T: Numerated + LowerBounded> From<RangeToInclusive<T>> for Interval<T> {
    fn from(range: RangeToInclusive<T>) -> Self {
        let (start, end) = (T::min_value(), range.end);
        debug_assert!(start <= end, "`T: LowerBounded` impl error");
        Self { start, end }
    }
}

impl<T: Numerated + UpperBounded + LowerBounded> From<RangeFull> for Interval<T> {
    fn from(_: RangeFull) -> Self {
        let (start, end) = (T::min_value(), T::max_value());
        debug_assert!(start <= end, "`T: UpperBounded + LowerBounded` impl error");
        Self { start, end }
    }
}

/// Trying to make empty interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyRangeError;

/// Trying to make interval from range where start > end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncorrectRangeError;

/// Trying to make interval from range where start > end or empty range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryFromRangeError {
    /// Trying to make empty interval.
    EmptyRange,
    /// Trying to make interval start > end.
    IncorrectRange,
}

impl<T: Numerated + UpperBounded, I: Into<T::Bound>> TryFrom<RangeFrom<I>> for Interval<T> {
    type Error = EmptyRangeError;

    fn try_from(range: RangeFrom<I>) -> Result<Self, Self::Error> {
        match Into::<T::Bound>::into(range.start).unbound() {
            Some(start) => {
                let end = T::max_value();
                debug_assert!(start <= end, "`T: UpperBounded` impl error");
                Ok(Self { start, end })
            }
            None => Err(EmptyRangeError),
        }
    }
}

impl<T: Numerated + LowerBounded + UpperBounded, I: Into<T::Bound>> TryFrom<RangeTo<I>>
    for Interval<T>
{
    type Error = EmptyRangeError;

    fn try_from(range: RangeTo<I>) -> Result<Self, Self::Error> {
        let end: T::Bound = range.end.into();

        let Some(end) = end.unbound() else {
            return Ok(Self::from(..));
        };

        let start = T::min_value();
        end.dec_if_gt(start)
            .map(|end| {
                debug_assert!(start <= end, "`T: LowerBounded` impl error");
                Self { start, end }
            })
            .ok_or(EmptyRangeError)
    }
}

impl<T: Numerated> TryFrom<RangeInclusive<T>> for Interval<T> {
    type Error = IncorrectRangeError;

    fn try_from(range: RangeInclusive<T>) -> Result<Self, Self::Error> {
        let (start, end) = range.into_inner();
        (start <= end)
            .then_some(Self { start, end })
            .ok_or(IncorrectRangeError)
    }
}

impl<T, S, E> TryFrom<(S, E)> for Interval<T>
where
    T: Numerated + UpperBounded,
    S: Into<T::Bound>,
    E: Into<T::Bound>,
{
    type Error = TryFromRangeError;

    // NOTE: trying to make upper not inclusive interval `start..=end - 1`
    fn try_from((start, end): (S, E)) -> Result<Self, Self::Error> {
        let start: T::Bound = start.into();
        let end: T::Bound = end.into();

        match (start.unbound(), end.unbound()) {
            (None, None) => Err(TryFromRangeError::EmptyRange),
            (None, Some(_)) => Err(TryFromRangeError::IncorrectRange),
            (start, None) => Self::try_from(start..).map_err(|_| TryFromRangeError::EmptyRange),
            (Some(start), Some(end)) => Self::try_from_range(start..end),
        }
    }
}

impl<T: Numerated + UpperBounded, I: Into<T::Bound>> TryFrom<Range<I>> for Interval<T> {
    type Error = TryFromRangeError;

    fn try_from(range: Range<I>) -> Result<Self, Self::Error> {
        Self::try_from((range.start, range.end))
    }
}

/// Trying to make zero len or out of bounds interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewWithLenError {
    /// Trying to make zero len interval.
    ZeroLen,
    /// Trying to make out of bounds interval.
    OutOfBounds,
}

impl<T: Numerated + UpperBounded> Interval<T> {
    /// Returns interval `start..=start + len - 1` if it's possible.
    /// - if `len == None`, then it is supposed, that `len == T::Distance::max_value() + 1`.
    /// - if `start + len - 1` is out of `T`, then returns [`NewWithLenError::OutOfBounds`].
    /// - if `len == 0`, then returns [`NewWithLenError::ZeroLen`].
    pub fn with_len<S: Into<T::Bound>, L: Into<Option<T::Distance>>>(
        start: S,
        len: L,
    ) -> Result<Interval<T>, NewWithLenError> {
        let start: T::Bound = start.into();
        let len: Option<T::Distance> = len.into();
        match (start.unbound(), len) {
            (_, Some(len)) if len.is_zero() => Err(NewWithLenError::ZeroLen),
            (None, _) => Err(NewWithLenError::OutOfBounds),
            (Some(start), len) => {
                // subtraction `len - 1` is safe, because `len != 0`
                let distance = len
                    .map(|len| len - T::Distance::one())
                    .unwrap_or(T::Distance::max_value());
                start
                    .add_if_enclosed_by(distance, T::max_value())
                    .map(|end| {
                        debug_assert!(start <= end, "`T: Numerated` impl error");
                        Self { start, end }
                    })
                    .ok_or(NewWithLenError::OutOfBounds)
            }
        }
    }
}

impl<T: Numerated> Interval<T> {
    /// - If `self` contains `T::Distance::max_value() + 1` points, then returns [`None`].
    /// - Else returns `Some(a)`, where `a` is amount of elements in `self`.
    pub fn raw_len(&self) -> Option<T::Distance> {
        let (start, end) = self.into_parts();
        end.distance(start).checked_add(&T::Distance::one())
    }
}

impl<T: Numerated + LowerBounded + UpperBounded> Interval<T> {
    /// Returns `len: T::Distance` (amount of points in `self`) converting it to `T` point:
    /// - If length is bigger than `T` possible elements amount, then returns `T::Bound` __upper__ value.
    /// - Else returns as length corresponding `p: T::Bound`:
    /// ```text
    ///   { 1 -> T::Bound::from(T::min_value() + 1), 2 -> T::Bound::from(T::min_value() + 2), ... }
    /// ```
    pub fn len(&self) -> T::Bound {
        self.raw_len()
            .and_then(|raw_len| T::min_value().add_if_enclosed_by(raw_len, T::max_value()))
            .into()
    }
}

impl<T: Debug> Debug for Interval<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}..={:?}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn len() {
        assert_eq!(Interval::<u8>::try_from(1..7).unwrap().len(), 6);
        assert_eq!(Interval::<u8>::try_from(..1).unwrap().len(), 1);
        assert_eq!(Interval::<u8>::from(..=1).len(), 2);
        assert_eq!(Interval::<u8>::try_from(1..).unwrap().len(), 255);
        assert_eq!(Interval::<u8>::try_from(0..).unwrap().len(), None);
        assert_eq!(Interval::<u8>::from(..).len(), None);

        assert_eq!(Interval::<u8>::try_from(1..7).unwrap().raw_len(), Some(6));
        assert_eq!(Interval::<u8>::try_from(..1).unwrap().raw_len(), Some(1));
        assert_eq!(Interval::<u8>::from(..=1).raw_len(), Some(2));
        assert_eq!(Interval::<u8>::try_from(1..).unwrap().raw_len(), Some(255));
        assert_eq!(Interval::<u8>::try_from(0..).unwrap().raw_len(), None);
        assert_eq!(Interval::<u8>::from(..).raw_len(), None);

        assert_eq!(Interval::<i8>::try_from(-1..1).unwrap().len(), -126); // corresponds to 2 numeration
        assert_eq!(Interval::<i8>::from(..=1).len(), 2); // corresponds to 130 numeration
        assert_eq!(Interval::<i8>::try_from(..1).unwrap().len(), 1); // corresponds to 129 numeration
        assert_eq!(Interval::<i8>::try_from(1..).unwrap().len(), -1); // corresponds to 127 numeration
        assert_eq!(Interval::<i8>::from(..).len(), None); // corresponds to 256 numeration

        assert_eq!(Interval::<i8>::try_from(-1..1).unwrap().raw_len(), Some(2));
        assert_eq!(Interval::<i8>::try_from(..1).unwrap().raw_len(), Some(129));
        assert_eq!(Interval::<i8>::from(..=1).raw_len(), Some(130));
        assert_eq!(Interval::<i8>::try_from(1..).unwrap().raw_len(), Some(127));
        assert_eq!(Interval::<i8>::from(..).raw_len(), None);
    }

    #[test]
    fn count_from() {
        assert_eq!(Interval::<u8>::with_len(0, 100).unwrap(), 0..=99);
        assert_eq!(Interval::<u8>::with_len(0, 255).unwrap(), 0..=254);
        assert_eq!(Interval::<u8>::with_len(0, None).unwrap(), 0..=255);
        assert_eq!(Interval::<u8>::with_len(1, 255).unwrap(), 1..=255);
        assert_eq!(Interval::<u8>::with_len(0, 1).unwrap(), 0..=0);
    }
}
