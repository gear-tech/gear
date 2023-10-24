use crate::{Interval, IntoIntervalError, NotEmptyInterval, Numerated};
use alloc::{collections::BTreeMap, fmt, fmt::Debug, vec::Vec};
use core::{fmt::Formatter, ops::RangeInclusive};
use num_traits::{
    bounds::{LowerBounded, UpperBounded},
    CheckedAdd, Zero,
};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

#[derive(Clone, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub struct Drops<T> {
    inner: BTreeMap<T, T>,
}

impl<T: Numerated> Default for Drops<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Debug> Debug for Drops<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}",
            self.inner.iter().map(|(s, e)| s..=e).collect::<Vec<_>>()
        )
    }
}

impl<T: Numerated> Drops<T> {
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = NotEmptyInterval<T>> + '_ {
        // `Self` guaranties, that contains only `start` <= `end`
        self.inner
            .iter()
            .map(|(&start, &end)| NotEmptyInterval::<T>::new_unchecked(start, end))
    }

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

    pub fn try_contains<I>(&self, interval: I) -> Option<bool>
    where
        I: TryInto<Interval<T>>,
        I::Error: Into<IntoIntervalError>,
    {
        let interval: Interval<T> = interval.try_into().ok()?;
        Some(self.contains(interval))
    }

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

        let mut iter = if let Some(end) = end.inc_if_lt(last) {
            self.inner.range(..=end)
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

        // In this point `start` < `right_start` <= `end`, so in any cases it will be removed.
        self.inner.remove(&right_start);

        let end = right_end.max(end);

        let Some((left_start, left_end)) = left_interval else {
            self.inner.insert(start, end);
            return;
        };

        debug_assert!(left_end < right_start);
        debug_assert!(left_start <= start);
        let Some(left_end) = left_end.inc_if_lt(right_start) else {
            unreachable!("`T: AsNumeric` impl error: if left_end < right_start, then left_end.inc() must return Some(_)");
        };

        if left_end >= start {
            self.inner.insert(left_start, end);
        } else {
            self.inner.insert(start, end);
        }
    }

    pub fn try_insert<I: TryInto<Interval<T>>>(&mut self, interval: I) -> Result<(), I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        self.insert(interval);
        Ok(())
    }

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
                unreachable!("`T: AsNumeric` impl error: if left_start < start, then start.dec() must return Some(_)");
            }
        }
    }

    pub fn try_remove<I: TryInto<Interval<T>>>(&mut self, interval: I) -> Result<(), I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        self.remove(interval);
        Ok(())
    }

    pub fn voids<I: Into<Interval<T>>>(
        &self,
        interval: I,
    ) -> VoidsIterator<T, impl Iterator<Item = NotEmptyInterval<T>> + '_> {
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
            // `Self` guaranties, that it contains only `start` <= `end`.
            NotEmptyInterval::new_unchecked(start, end)
        });

        // Already checked, that `start` <= `end`.
        let interval = NotEmptyInterval::new_unchecked(start, end);

        VoidsIterator {
            inner: Some((iter, interval)),
        }
    }

    pub fn try_voids<I: TryInto<Interval<T>>>(
        &self,
        interval: I,
    ) -> Result<VoidsIterator<T, impl Iterator<Item = NotEmptyInterval<T>> + '_>, I::Error> {
        let interval: Interval<T> = interval.try_into()?;
        Ok(self.voids(interval))
    }

    pub fn and_not_iter<'a: 'b, 'b: 'a>(
        &'a self,
        other: &'b Self,
    ) -> impl Iterator<Item = NotEmptyInterval<T>> + '_ {
        AndNotIterator {
            iter1: self.iter(),
            iter2: other.iter(),
            interval1: None,
            interval2: None,
        }
    }

    fn into_start_end<I: Into<Interval<T>>>(interval: I) -> Option<(T, T)> {
        Into::<Interval<T>>::into(interval).into_inner()
    }

    pub fn end(&self) -> Option<T> {
        self.inner.iter().next_back().map(|(_, &e)| e)
    }

    pub fn start(&self) -> Option<T> {
        self.inner.iter().next().map(|(&s, _)| s)
    }

    pub fn intervals_amount(&self) -> usize {
        self.inner.len()
    }

    // +_+_+ TODO: may be better to store points amount as field.
    /// Number of points in tree set.
    pub fn points_amount(&self) -> Option<T::N> {
        let mut res = T::N::zero();
        for interval in self.iter() {
            res = res.checked_add(&interval.raw_size()?)?;
        }
        Some(res)
    }

    pub fn points_iter(&self) -> impl Iterator<Item = T> + '_ {
        // `Self` guaranties, that contains only `end` >= `start`
        self.inner
            .iter()
            .flat_map(|(&s, &e)| NotEmptyInterval::new_unchecked(s, e).iter())
    }

    pub fn to_vec(&self) -> Vec<RangeInclusive<T>> {
        self.iter().map(Into::into).collect()
    }
}

impl<T: Numerated + LowerBounded + UpperBounded> Drops<T> {
    // TODO: optimize
    pub fn complement(&self) -> Self {
        let mut res = Drops::<T>::new();
        let mut start: Option<T> = None;
        for interval in self.iter() {
            if let Some(start) = start {
                // `start` < `interval.start()` cause all intervals in tree are sorted.
                debug_assert!(start < interval.start());
                let start = start.inc_if_lt(interval.start()).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each x: T, y: T, x < y => x.inc_if_lt(y) == Some(_)");
                });

                // `Self` guaranties, that between each two intervals void exists.
                debug_assert!(start < interval.start());
                let end = interval.start().dec_if_gt(start).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y => x.dec_if_gt(y) == Some(_)");
                });

                res.insert(NotEmptyInterval::new(start, end).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y => x.dec_if_gt(y) >= y");
                }));
            } else {
                res.insert(..interval.start());
            }
            start = Some(interval.end());
        }

        if let Some(start) = start {
            if let Some(start) = start.inc_if_lt(T::max_value()) {
                res.insert(start..);
            }
        } else {
            res.insert(..);
        }

        res
    }

    // TODO: optimize
    pub fn crop<I: Into<Interval<T>>>(&mut self, interval: I) {
        let Some((start, end)) = Self::into_start_end(interval) else {
            // Empty interval - just clear.
            self.inner.clear();
            return;
        };

        self.remove(..start);
        if let Some(end) = end.inc_if_lt(T::max_value()) {
            self.remove(end..);
        }
    }
}

pub struct VoidsIterator<T: Numerated, I: Iterator<Item = NotEmptyInterval<T>>> {
    inner: Option<(I, NotEmptyInterval<T>)>,
}

impl<T: Numerated, I: Iterator<Item = NotEmptyInterval<T>>> Iterator for VoidsIterator<T, I> {
    type Item = NotEmptyInterval<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (iter, interval) = self.inner.as_mut()?;
        if let Some(next) = iter.next() {
            let (start, end) = next.into_inner();

            // Guaranties by tree: between two intervals always exists void.
            debug_assert!(interval.start() < start);

            let void_end = start.dec_if_gt(interval.start()).unwrap_or_else(|| {
                unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y => x.dec_if_gt(y) == Some(_)");
            });

            let res = NotEmptyInterval::new(interval.start(), void_end);
            if res.is_none() {
                unreachable!(
                    "`T: Numerated` impl error: for each x: T, y: T, x > y => x.dec_if_gt(y) >= y"
                );
            }

            if let Some(new_start) = end.inc_if_lt(interval.end()) {
                *interval = NotEmptyInterval::new(new_start, interval.end()).unwrap_or_else(|| {
                    unreachable!("`T: Numerated` impl error: for each x: T, y: T, x < y => x.inc_if_lt(y) <= y");
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

// +_+_+ TODO: make stress testing
pub struct AndNotIterator<
    T: Numerated,
    I1: Iterator<Item = NotEmptyInterval<T>>,
    I2: Iterator<Item = NotEmptyInterval<T>>,
> {
    iter1: I1,
    iter2: I2,
    interval1: Option<NotEmptyInterval<T>>,
    interval2: Option<NotEmptyInterval<T>>,
}

impl<
        T: Numerated,
        I1: Iterator<Item = NotEmptyInterval<T>>,
        I2: Iterator<Item = NotEmptyInterval<T>>,
    > Iterator for AndNotIterator<T, I1, I2>
{
    type Item = NotEmptyInterval<T>;

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
                    self.interval1 = NotEmptyInterval::new(new_start, interval1.end());
                    if self.interval1.is_none() {
                        unreachable!("`T: Numerated` impl error: for each x: T, y: T, x < y => x.inc_if_lt(y) <= y");
                    }
                } else if interval1.end() == interval2.end() {
                    self.interval1 = None;
                    self.interval2 = None;
                } else {
                    self.interval1 = None;
                }

                if let Some(new_end) = interval2.start().dec_if_gt(interval1.start()) {
                    let res = NotEmptyInterval::new(interval1.start(), new_end);
                    if res.is_none() {
                        unreachable!("`T: Numerated` impl error: for each x: T, y: T, x > y => x.dec_if_gt(y) >= y");
                    }
                    return res;
                } else {
                    continue;
                }
            }
        }
    }
}

impl<T: Numerated, D: Into<Interval<T>>> FromIterator<D> for Drops<T> {
    fn from_iter<I: IntoIterator<Item = D>>(iter: I) -> Self {
        let mut drops = Self::new();
        for interval in iter {
            drops.insert(interval);
        }
        drops
    }
}
