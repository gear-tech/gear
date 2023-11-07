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

//! [NotEmptyInterval], [Interval] implementations.

use core::{
    fmt::{self, Debug, Display, Formatter},
    ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive},
};
use num_traits::{
    bounds::{LowerBounded, UpperBounded},
    CheckedAdd, One, Zero,
};

use crate::{
    numerated::{BoundValue, Numerated},
    Bound,
};

/// Describes not empty interval start..=end.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NotEmptyInterval<T> {
    start: T,
    end: T,
}

/// Describes interval start..=end, where end can be None,
/// which means that interval is empty.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Interval<T> {
    start: T,
    end: Option<T>,
}

impl<T: Numerated> From<NotEmptyInterval<T>> for (T, T) {
    fn from(interval: NotEmptyInterval<T>) -> (T, T) {
        (interval.start, interval.end)
    }
}

impl<T: Numerated> From<NotEmptyInterval<T>> for RangeInclusive<T> {
    fn from(interval: NotEmptyInterval<T>) -> Self {
        interval.start..=interval.end
    }
}

impl<T: Numerated> NotEmptyInterval<T> {
    /// Creates new interval start..=end with checks only in debug mode.
    /// # Safety
    /// Unsafe, because allows to create invalid interval.
    #[track_caller]
    pub unsafe fn new_unchecked(start: T, end: T) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    /// Creates new interval start..=end if start <= end, else returns None.
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

    /// Converts to [Interval], which implements iterator.
    pub fn iter(&self) -> Interval<T> {
        (*self).into()
    }

    /// Into (start, end)
    pub fn into_inner(self) -> (T, T) {
        self.into()
    }
}

impl<T: Numerated> Interval<T> {
    /// Creates new interval start..end if start <= end, else returns None.
    /// If start == end, then returns empty interval.
    pub fn new<S: Into<T::B>, E: Into<T::B>>(start: S, end: E) -> Option<Self> {
        Self::try_from((start, end)).ok()
    }
    /// Interval start.
    /// If interval is empty, then returns any existed `T` point,
    /// which user set when creates this interval.
    /// If interval is not empty, then returns the smallest value inside interval.
    pub fn start(&self) -> T {
        self.start
    }
    /// Returns whether interval is empty.
    pub fn is_empty(&self) -> bool {
        self.end.is_none()
    }
    /// Tries to convert into not empty interval.
    pub fn into_not_empty(self) -> Option<NotEmptyInterval<T>> {
        self.end.map(|end| unsafe {
            // Guaranteed by `Self` that start <= end
            NotEmptyInterval::new_unchecked(self.start, end)
        })
    }
    /// Tries to convert into (start, end).
    pub fn into_inner(self) -> Option<(T, T)> {
        self.into_not_empty().map(Into::into)
    }
    /// Tries to convert into range inclusive.
    pub fn into_range_inclusive(self) -> Option<RangeInclusive<T>> {
        self.into_not_empty().map(Into::into)
    }
}

impl<T: Numerated> From<NotEmptyInterval<T>> for Interval<T> {
    fn from(interval: NotEmptyInterval<T>) -> Self {
        Self {
            start: interval.start,
            end: Some(interval.end),
        }
    }
}

impl<T: Numerated> Iterator for Interval<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((start, end)) = self.into_inner() {
            if start == end {
                self.end = None;
                Some(start)
            } else {
                // Guaranteed by `Self`
                debug_assert!(start < end);

                let result = start;
                let start = start.inc_if_lt(end).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each s: T, e: T, e > s => s.inc_if_lt(e) == Some(_)")
                });
                *self = Interval::try_from(start..=end).unwrap_or_else(|_| {
                    unreachable!("`T: Numerated` impl error: for each s: T, e: T, e > s => s.inc_if_lt(e) <= e")
                });
                Some(result)
            }
        } else {
            None
        }
    }
}

impl<T: Numerated> From<T> for Interval<T> {
    fn from(point: T) -> Self {
        unsafe {
            // Safe cause point <= point
            NotEmptyInterval::new_unchecked(point, point).into()
        }
    }
}

impl<T: Numerated> From<&T> for Interval<T> {
    fn from(point: &T) -> Self {
        Self::from(*point)
    }
}

impl<T: Numerated + LowerBounded> From<RangeToInclusive<T>> for Interval<T> {
    fn from(range: RangeToInclusive<T>) -> Self {
        NotEmptyInterval::new(T::min_value(), range.end)
            .unwrap_or_else(|| {
                unreachable!(
                    "`T: LowerBounded` impl error: for each x: T must be T::min_value() <= x"
                )
            })
            .into()
    }
}

impl<T: Numerated + UpperBounded, I: Into<T::B>> From<RangeFrom<I>> for Interval<T> {
    fn from(range: RangeFrom<I>) -> Self {
        let start: T::B = range.start.into();
        match start.unbound() {
            BoundValue::Value(start) => NotEmptyInterval::new(start, T::max_value())
                .unwrap_or_else(|| {
                    unreachable!(
                        "`T: UpperBounded` impl error: for each x: T must be x <= T::max_value()"
                    )
                })
                .into(),
            BoundValue::Upper(start) => Self { start, end: None },
        }
    }
}

impl<T: Numerated + UpperBounded + LowerBounded> From<RangeFull> for Interval<T> {
    fn from(_: RangeFull) -> Self {
        NotEmptyInterval::new(T::min_value(), T::max_value()).unwrap_or_else(|| {
            unreachable!("`T: UpperBounded + LowerBounded` impl error: must be T::min_value() <= T::max_value()")
        }).into()
    }
}

impl<T: Numerated + LowerBounded + UpperBounded, I: Into<T::B>> From<RangeTo<I>> for Interval<T> {
    fn from(range: RangeTo<I>) -> Self {
        let end: T::B = range.end.into();
        let start = T::min_value();
        match end.unbound() {
            BoundValue::Value(end) => {
                debug_assert!(end >= T::min_value());
                let end = end.dec_if_gt(T::min_value());
                Self { start, end }
            }
            BoundValue::Upper(end) => Self {
                start,
                end: Some(end),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IntoIntervalError;

impl<T: Numerated, S: Into<T::B>, E: Into<T::B>> TryFrom<(S, E)> for Interval<T> {
    type Error = IntoIntervalError;

    fn try_from((start, end): (S, E)) -> Result<Self, Self::Error> {
        use BoundValue::*;

        let start: T::B = start.into();
        let end: T::B = end.into();

        match (start.unbound(), end.unbound()) {
            (Upper(start), Upper(_)) => Ok(Self { start, end: None }),
            (Upper(_), Value(_)) => Err(IntoIntervalError),
            (Value(start), Upper(end)) => Ok(Self {
                start,
                end: Some(end),
            }),
            (Value(start), Value(end)) => {
                if let Some(end) = end.dec_if_gt(start) {
                    debug_assert!(start <= end);
                    Ok(Self {
                        start,
                        end: Some(end),
                    })
                } else if start == end {
                    Ok(Self { start, end: None })
                } else {
                    Err(IntoIntervalError)
                }
            }
        }
    }
}

impl<T: Numerated, I: Into<T::B>> TryFrom<Range<I>> for Interval<T> {
    type Error = IntoIntervalError;

    fn try_from(range: Range<I>) -> Result<Self, Self::Error> {
        Self::try_from((range.start, range.end))
    }
}

impl<T: Numerated> TryFrom<RangeInclusive<T>> for Interval<T> {
    type Error = IntoIntervalError;

    fn try_from(range: RangeInclusive<T>) -> Result<Self, Self::Error> {
        let (start, end) = range.into_inner();
        NotEmptyInterval::new(start, end)
            .ok_or(IntoIntervalError)
            .map(Into::into)
    }
}

impl<T: Numerated> NotEmptyInterval<T> {
    /// Returns amount of elements in interval in `T::N` if it's possible.
    /// None means that interval size is bigger, than `T::N::max_value()`.
    pub fn raw_size(&self) -> Option<T::N> {
        let (start, end) = self.into_inner();

        // Guarantied by NotEmptyInterval
        debug_assert!(start <= end);

        let distance = end.distance(start).unwrap_or_else(|| {
            unreachable!(
                "`T: Numerated` impl error: for each s: T, e: T, e >= s => e.distance(s) == Some(_)"
            )
        });

        distance.checked_add(&T::N::one())
    }
}

impl<T: Numerated + LowerBounded + UpperBounded> NotEmptyInterval<T> {
    /// Returns size of interval in `T` if it's possible.
    /// If interval size is bigger than `T` possible elements amount, then returns `None`.
    /// If interval size is equal to some `T::N`, then returns `T` of corresponding numeration.
    pub fn size(&self) -> Option<T> {
        let raw_size = self.raw_size()?;
        let size = T::min_value()
            .add_if_enclosed_by(raw_size, T::max_value())
            .unwrap_or_else(|| unreachable!("`T: Numerated` impl error"));
        Some(size)
    }
}

impl<T: Numerated> Interval<T> {
    /// Returns amount of elements in interval in `T::N` if it's possible.
    /// None means that interval size is bigger, than `T::N::max_value()`.
    /// If interval is empty, then returns `Some(T::min_value())`,
    /// which is actually equal to `Some(T::zero())`.
    pub fn raw_size(&self) -> Option<T::N> {
        let Some(interval) = self.into_not_empty() else {
            return Some(T::N::min_value());
        };

        interval.raw_size()
    }
}

impl<T: Numerated + LowerBounded + UpperBounded> Interval<T> {
    /// Returns size of interval in `T` if it's possible.
    /// If interval is empty, then returns `Some(T::min_value())`.
    /// If interval size is bigger than `T` possible elements amount, then returns `None`.
    /// If interval size is equal to some `T::N`, then returns `T` of corresponding numeration.
    pub fn size(&self) -> Option<T> {
        let Some(interval) = self.into_not_empty() else {
            return Some(T::min_value());
        };

        interval.size()
    }
}

impl<T: Numerated + UpperBounded> Interval<T> {
    /// Returns interval [`start`..`start` + `count`) if it's possible.
    /// Size of result interval is equal to `count`.
    /// If `count` is None, then supposed that interval size must be `T::N::max_value()`.
    /// If `start` + `count` - 1 is out of `T`, then returns `None`.
    /// If `count` is zero, then returns empty interval.
    pub fn count_from<S: Into<T::B>, C: Into<Option<T::N>>>(start: S, count: C) -> Option<Self> {
        use BoundValue::*;
        let start: T::B = start.into();
        let count: Option<T::N> = count.into();
        match (start.unbound(), count) {
            (Value(start), Some(c)) | (Upper(start), Some(c)) if c == T::N::zero() => {
                Some(Self { start, end: None })
            }
            (Upper(_), _) => None,
            (Value(s), c) => {
                // subtraction is safe, because c != 0
                let c = c.map(|c| c - T::N::one()).unwrap_or(T::N::max_value());
                s.add_if_enclosed_by(c, T::max_value())
                    .map(|e| NotEmptyInterval::new(s, e).unwrap_or_else(|| {
                        unreachable!("`T: Numerated` impl error: for each s: T, c: T::N => s.add_if_between(c, _) >= s")
                    }).into())
            }
        }
    }
}

impl<T: Debug> Debug for NotEmptyInterval<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({:?}..={:?})", self.start, self.end)
    }
}

impl<T: Display> Display for NotEmptyInterval<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({}..={})", self.start, self.end)
    }
}

impl<T: Debug> Debug for Interval<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "({:?}..={:?})", self.start, self.end)
    }
}

impl<T: Display> Display for Interval<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(end) = self.end.as_ref() {
            write!(f, "({}..={})", self.start, end)
        } else {
            write!(f, "∅({})", self.start)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size() {
        assert_eq!(Interval::<u8>::try_from(11..111).unwrap().size(), Some(100),);
        assert_eq!(Interval::<u8>::try_from(..1).unwrap().size(), Some(1),);
        assert_eq!(Interval::<u8>::from(..=1).size(), Some(2));
        assert_eq!(Interval::<u8>::from(1..).size(), Some(255));
        assert_eq!(Interval::<u8>::from(0..).size(), None);
        assert_eq!(Interval::<u8>::from(..).size(), None);
        assert_eq!(Interval::<u8>::try_from(1..1).unwrap().size(), Some(0));

        assert_eq!(
            Interval::<u8>::try_from(11..111).unwrap().raw_size(),
            Some(100),
        );
        assert_eq!(Interval::<u8>::try_from(..1).unwrap().raw_size(), Some(1),);
        assert_eq!(Interval::<u8>::from(..=1).raw_size(), Some(2));
        assert_eq!(Interval::<u8>::from(1..).raw_size(), Some(255));
        assert_eq!(Interval::<u8>::from(0..).raw_size(), None);
        assert_eq!(Interval::<u8>::from(..).raw_size(), None);
        assert_eq!(Interval::<u8>::try_from(1..1).unwrap().raw_size(), Some(0));

        assert_eq!(Interval::<i8>::try_from(-1..99).unwrap().size(), Some(-28)); // corresponds to 100 numeration
        assert_eq!(Interval::<i8>::try_from(..1).unwrap().size(), Some(1)); // corresponds to 129 numeration
        assert_eq!(Interval::<i8>::from(..=1).size(), Some(2)); // corresponds to 130 numeration
        assert_eq!(Interval::<i8>::from(1..).size(), Some(-1)); // corresponds to 127 numeration
        assert_eq!(Interval::<i8>::from(0..).size(), Some(0)); // corresponds to 128 numeration
        assert_eq!(Interval::<i8>::from(..).size(), None); // corresponds to 256 numeration
        assert_eq!(Interval::<i8>::try_from(1..1).unwrap().size(), Some(-128)); // corresponds to 0 numeration

        assert_eq!(
            Interval::<i8>::try_from(-1..99).unwrap().raw_size(),
            Some(100)
        );
        assert_eq!(Interval::<i8>::try_from(..1).unwrap().raw_size(), Some(129));
        assert_eq!(Interval::<i8>::from(..=1).raw_size(), Some(130));
        assert_eq!(Interval::<i8>::from(1..).raw_size(), Some(127));
        assert_eq!(Interval::<i8>::from(0..).raw_size(), Some(128));
        assert_eq!(Interval::<i8>::from(..).raw_size(), None);
        assert_eq!(Interval::<i8>::try_from(1..1).unwrap().raw_size(), Some(0));
    }

    #[test]
    fn count_from() {
        assert_eq!(
            Interval::<u8>::count_from(0, 100).and_then(Interval::into_range_inclusive),
            Some(0..=99)
        );
        assert_eq!(
            Interval::<u8>::count_from(0, 255).and_then(Interval::into_range_inclusive),
            Some(0..=254)
        );
        assert_eq!(
            Interval::<u8>::count_from(0, None).and_then(Interval::into_range_inclusive),
            Some(0..=255)
        );
        assert_eq!(
            Interval::<u8>::count_from(1, 255).and_then(Interval::into_range_inclusive),
            Some(1..=255)
        );

        assert!(Interval::<u8>::count_from(1, 0).unwrap().is_empty());
        assert_eq!(Interval::<u8>::count_from(1, None), None);
        assert_eq!(Interval::<u8>::count_from(2, 255), None);
    }
}
