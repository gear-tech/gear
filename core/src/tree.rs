
// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Intervals tree with limited amount of intervals.
use core::{
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
    ops::RangeInclusive,
};

use numerated::{
    interval::Interval,
    iterators::{IntervalIterator, VoidsIterator},
    Numerated,
};
use parity_scale_codec::{Decode, Encode, EncodeLike, MaxEncodedLen, Output};
use scale_info::TypeInfo;

use super::{buffer::LimitedVec, pages::numerated::tree::IntervalsTree};

/// A tree that can only store up to `N` intervals.
///
/// Internally uses [`IntervalsTree`], just adds assertions
/// that we do not insert more than `N` elements.
#[derive(Clone, PartialEq, Eq, TypeInfo, Decode)]
pub struct LimitedIntervalsTree<T, E, const N: usize> {
    inner: IntervalsTree<T>,
    marker: PhantomData<E>,
}

impl<T: Copy, E, const N: usize> LimitedIntervalsTree<T, E, N> {
    /// Creates a new empty tree.
    pub const fn new() -> Self {
        Self {
            inner: IntervalsTree::new(),
            marker: PhantomData,
        }
    }

    /// See [IntervalsTree::intervals_amount]
    pub fn intervals_amount(&self) -> usize {
        self.inner.intervals_amount()
    }

    /// Returns underlying intervals tree.
    pub fn into_intervals_tree(self) -> IntervalsTree<T> {
        self.inner
    }

    /// Returns underlying intervals tree reference.
    pub fn inner(&self) -> &IntervalsTree<T> {
        &self.inner
    }

    /// Returns mutable reference to underlying intervals tree.
    pub fn inner_mut(&mut self) -> &mut IntervalsTree<T> {
        &mut self.inner
    }
}

impl<T: Copy + Ord, E, const N: usize> LimitedIntervalsTree<T, E, N> {
    /// Returns the biggest point in tree.
    pub fn end(&self) -> Option<T> {
        self.inner.end()
    }
    /// Returns the smallest point in tree.
    pub fn start(&self) -> Option<T> {
        self.inner.start()
    }
}

impl<T: Copy + Ord, E, const N: usize> Default for LimitedIntervalsTree<T, E, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Debug + Numerated, E, const N: usize> Debug for LimitedIntervalsTree<T, E, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LimitedIntervalsTree")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T: Numerated, E: Default, const N: usize> LimitedIntervalsTree<T, E, N> {
    /// Returns iterator over all intervals in tree.
    pub fn iter(&self) -> impl Iterator<Item = Interval<T>> + '_ {
        self.inner.iter()
    }

    /// Returns true if for each `p` ∈ `interval` ⇒ `p` ∈ `self`, otherwise returns false.
    pub fn contains<I: Into<IntervalIterator<T>>>(&self, interval: I) -> bool {
        self.inner.contains(interval)
    }
    /// Insert interval into tree.
    /// - if `interval` is empty, then nothing will be inserted.
    /// - if `interval` is not empty, then after insertion: for each `p` ∈ `interval` ⇒ `p` ∈ `self`.
    ///
    /// Complexity: `O(m * log(n))`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn insert<I: Into<IntervalIterator<T>>>(&mut self, interval: I) -> Result<bool, E> {
        if self.inner.intervals_amount() >= N {
            return Err(E::default());
        }
        Ok(self.inner.insert(interval))
    }

    /// Remove `interval` from tree.
    /// - if `interval` is empty, then nothing will be removed.
    /// - if `interval` is not empty, then after removing: for each `p` ∈ `interval` ⇒ `p` ∉ `self`.
    ///
    /// Complexity: `O(m * log(n))`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `self` ⋂ `interval`
    pub fn remove<I: Into<IntervalIterator<T>>>(&mut self, interval: I) -> bool {
        self.inner.remove(interval)
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
        self.inner.voids(interval)
    }
    /// Returns iterator over intervals, which consist of points `p: T`,
    /// where each `p` ∈ `self` and `p` ∉ `other`.
    ///
    /// Iterating complexity: `O(n + m)`, where
    /// - `n` is amount of intervals in `self`
    /// - `m` is amount of intervals in `other`
    pub fn difference<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Interval<T>> + 'a {
        self.inner.difference(&other.inner)
    }
    /// Number of points in tree set.
    ///
    /// Complexity: `O(n)`, where `n` is amount of intervals in `self`.
    pub fn points_amount(&self) -> Option<T::Distance> {
        self.inner.points_amount()
    }
    /// Iterator over all points in tree set.
    pub fn points_iter(&self) -> impl Iterator<Item = T> + '_ {
        self.inner.points_iter()
    }
    /// Convert tree to vector of inclusive ranges limited by `N` elements.
    pub fn to_vec(&self) -> Result<LimitedVec<RangeInclusive<T>, E, N>, E> {
        LimitedVec::try_from(self.inner.to_vec())
    }
}

impl<T: MaxEncodedLen, E, const N: usize> MaxEncodedLen for LimitedIntervalsTree<T, E, N> {
    fn max_encoded_len() -> usize {
        N * T::max_encoded_len()
    }
}

impl<T: Encode, E, const N: usize> Encode for LimitedIntervalsTree<T, E, N> {
    fn encode(&self) -> alloc::vec::Vec<u8> {
        self.inner.encode()
    }

    fn encode_to<U: Output + ?Sized>(&self, dest: &mut U) {
        self.inner.encode_to(dest)
    }
}

impl<T: Copy, E: Default, const N: usize> TryFrom<IntervalsTree<T>>
    for LimitedIntervalsTree<T, E, N>
{
    type Error = E;
    fn try_from(inner: IntervalsTree<T>) -> Result<Self, Self::Error> {
        (inner.intervals_amount() <= N)
            .then(|| Self {
                inner,
                marker: PhantomData,
            })
            .ok_or_else(E::default)
    }
}

impl<T: Encode, E, const N: usize> EncodeLike<IntervalsTree<T>> for LimitedIntervalsTree<T, E, N> {}
