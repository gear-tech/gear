// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Module for map's iterator primitives.
//!
//! Map's iterators primitives declares the ability
//! to iter through defined generic `Item` over the map
//! with specified (associated) types of iterators
//! for drain or just iter elements.
//!
//! `DrainIter` used for element's removal
//! on each iteration, while `Iter` used for
//! just checking them.

use super::TransposeCallback;
use core::marker::PhantomData;

/// Represents iterable logic for double key maps
/// (Key1 -> Key2 -> Value).
///
/// Returns the iterators over specified (associated)
/// type of the first key's items.
pub trait IterableByKeyMap<Item> {
    /// Map's first key type.
    type Key;
    /// Removal iterator type.
    type DrainIter: Iterator<Item = Item>;
    /// Getting iterator type.
    type Iter: Iterator<Item = Item>;

    /// Creates the removal iterator over double map Items.
    fn drain_key(key: Self::Key) -> Self::DrainIter;
    /// Creates the getting iterator over double map Items.
    fn iter_key(key: Self::Key) -> Self::Iter;
}

/// Represents iterable logic for single key maps
/// (Key -> Value).
pub trait IterableMap<Item> {
    /// Removal iterator type.
    type DrainIter: Iterator<Item = Item>;
    /// Getting iterator type.
    type Iter: Iterator<Item = Item>;

    /// Creates the removal iterator over map Items.
    fn drain() -> Self::DrainIter;
    /// Creates the getting iterator over map Items.
    fn iter() -> Self::Iter;
}

/// Represents iterable over second keys logic for double key maps
/// (Key1 -> Key2 -> Value).
///
/// Returns the iterators over specified (associated)
/// type of the second map keys by given first key.
pub trait KeyIterableByKeyMap {
    /// Map's first key type.
    type Key1;
    /// Map's second key type.
    type Key2;
    /// Removal iterator type.
    type DrainIter: Iterator<Item = Self::Key2>;
    /// Getting iterator type.
    type Iter: Iterator<Item = Self::Key2>;

    /// Creates the removal iterator over double map Items.
    fn drain_prefix_keys(key: Self::Key1) -> Self::DrainIter;
    /// Creates the getting iterator over double map Items.
    fn iter_prefix_keys(key: Self::Key1) -> Self::Iter;
}

/// Transpose callback for getting first element of tuple.
pub struct GetFirstPos;

// `TransposeCallback` implementation for tuple with two elements.
impl<K, V> TransposeCallback<(K, V), K> for GetFirstPos {
    fn call(arg: (K, V)) -> K {
        arg.0
    }
}

// `TransposeCallback` implementation for tuple with three elements.
impl<K1, K2, V> TransposeCallback<(K1, K2, V), K1> for GetFirstPos {
    fn call(arg: (K1, K2, V)) -> K1 {
        arg.0
    }
}

/// Transpose callback for getting second element of tuple.
pub struct GetSecondPos;

// `TransposeCallback` implementation for tuple with two elements.
impl<K, V> TransposeCallback<(K, V), V> for GetSecondPos {
    fn call(arg: (K, V)) -> V {
        arg.1
    }
}

// `TransposeCallback` implementation for tuple with three elements.
impl<K1, K2, V> TransposeCallback<(K1, K2, V), K2> for GetSecondPos {
    fn call(arg: (K1, K2, V)) -> K2 {
        arg.1
    }
}

/// Transpose callback for getting third element of tuple.
pub struct GetThirdPos;

// `TransposeCallback` implementation for tuple with three elements.
impl<K1, K2, V> TransposeCallback<(K1, K2, V), V> for GetThirdPos {
    fn call(arg: (K1, K2, V)) -> V {
        arg.2
    }
}

/// Represents wrapper for any iterator with ability
/// to transpose `.next()` result.
pub struct IteratorWrap<I, Item = <I as Iterator>::Item, TC = ()>(I, PhantomData<(Item, TC)>)
where
    I: Iterator,
    TC: TransposeCallback<I::Item, Item>;

// Implementation of `From` for any iterator.
impl<I, Item, TC> From<I> for IteratorWrap<I, Item, TC>
where
    I: Iterator,
    TC: TransposeCallback<I::Item, Item>,
{
    fn from(iterator: I) -> Self {
        Self(iterator, PhantomData)
    }
}

// Implementation of `Iterator` itself for the wrapper
// based on inner iterator and transpose callback.
impl<I, Item, TC> Iterator for IteratorWrap<I, Item, TC>
where
    I: Iterator,
    TC: TransposeCallback<I::Item, Item>,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(TC::call)
    }
}
