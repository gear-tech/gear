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

/// Represents value iterator wrap for (key, value) iterator.
pub struct KeyValueIteratorWrap<K, V, I>(I, PhantomData<(K, V)>)
where
    I: Iterator<Item = (K, V)>;

// `From` implementation `KeyValueIteratorWrap` for soft iterator wrapping.
impl<K, V, I> From<I> for KeyValueIteratorWrap<K, V, I>
where
    I: Iterator<Item = (K, V)>,
{
    fn from(other: I) -> Self {
        Self(other, PhantomData)
    }
}

// `Iterator` implementation for `KeyValueIteratorWrap`.
impl<K, V, I> Iterator for KeyValueIteratorWrap<K, V, I>
where
    I: Iterator<Item = (K, V)>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|v| v.1)
    }
}

/// Represents value iterator wrap for (key1, key2, value) iterator.
pub struct KeysValueIteratorWrap<K1, K2, V, I>(I, PhantomData<(K1, K2, V)>)
where
    I: Iterator<Item = (K1, K2, V)>;

// `From` implementation `KeysValueIteratorWrap` for soft iterator wrapping.
impl<K1, K2, V, I> From<I> for KeysValueIteratorWrap<K1, K2, V, I>
where
    I: Iterator<Item = (K1, K2, V)>,
{
    fn from(other: I) -> Self {
        Self(other, PhantomData)
    }
}

// `Iterator` implementation for `KeysValueIteratorWrap`.
impl<K1, K2, V, I> Iterator for KeysValueIteratorWrap<K1, K2, V, I>
where
    I: Iterator<Item = (K1, K2, V)>,
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|v| v.2)
    }
}
