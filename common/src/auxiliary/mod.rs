// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Auxiliary implementations of the complex data structures,
//! which manage important for the gear runtime storages. These
//! implementations can be used in a non-wasm environment.

pub mod gas_provider;
pub mod mailbox;
pub mod task_pool;
pub mod waitlist;

use crate::storage::{
    Counted, CountedByKey, DoubleMapStorage, GetFirstPos, GetSecondPos, IterableByKeyMap,
    IteratorWrap, KeyIterableByKeyMap, MapStorage,
};
use alloc::collections::btree_map::{BTreeMap, Entry, IntoIter};

/// An "auxiliary" block number type.
pub type BlockNumber = u32;

/// Double key `BTreeMap`.
///
/// Basically is just a map of the map.
pub struct DoubleBTreeMap<K1, K2, V> {
    inner: BTreeMap<K1, BTreeMap<K2, V>>,
}

impl<K1, K2, V> DoubleBTreeMap<K1, K2, V> {
    /// Instantiate new empty double key map.
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    /// Returns `true` if the map contains a value for the specified keys.
    pub fn contains_keys(&self, key1: &K1, key2: &K2) -> bool
    where
        K1: Ord,
        K2: Ord,
    {
        self.inner
            .get(key1)
            .map(|map| map.contains_key(key2))
            .unwrap_or_default()
    }

    pub fn count_key(&self, key1: &K1) -> usize
    where
        K1: Ord,
    {
        self.inner
            .get(key1)
            .map(|key2_map| key2_map.len())
            .unwrap_or_default()
    }

    /// Returns a reference to the value corresponding to the keys.
    pub fn get(&self, key1: &K1, key2: &K2) -> Option<&V>
    where
        K1: Ord,
        K2: Ord,
    {
        self.inner.get(key1).and_then(|map| map.get(key2))
    }

    /// Inserts a value under provided keys in the map.
    pub fn insert(&mut self, key1: K1, key2: K2, value: V) -> Option<V>
    where
        K1: Ord,
        K2: Ord,
    {
        match self.inner.entry(key1) {
            Entry::Vacant(vacant) => {
                let mut map = BTreeMap::new();
                map.insert(key2, value);
                vacant.insert(map);

                None
            }
            Entry::Occupied(mut occupied) => occupied.get_mut().insert(key2, value),
        }
    }

    /// Removes keys from the map, returning the value at the keys if the keys
    /// were previously in the map.
    pub fn remove(&mut self, key1: K1, key2: K2) -> Option<V>
    where
        K1: Ord,
        K2: Ord,
    {
        self.inner.get_mut(&key1).and_then(|map| map.remove(&key2))
    }

    /// Clears the map, removing all elements.
    pub fn clear(&mut self) {
        self.inner.clear()
    }
}

// Iterator related impl
impl<K1, K2, V> DoubleBTreeMap<K1, K2, V> {
    pub fn iter_key(&self, key1: &K1) -> IntoIter<K2, V>
    where
        K1: Ord,
        K2: Clone,
        V: Clone,
    {
        self.inner
            .get(key1)
            .cloned()
            .map(|key2_map| key2_map.into_iter())
            .unwrap_or_default()
    }

    pub fn drain_key(&mut self, key1: &K1) -> IntoIter<K2, V>
    where
        K1: Ord,
    {
        self.inner
            .remove(key1)
            .map(|key2_map| key2_map.into_iter())
            .unwrap_or_default()
    }
}

impl<K1, K2, V> Default for DoubleBTreeMap<K1, K2, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// An auxiliary storage wrapper type.
///
/// Implements DoubleMapStorage and traits like [`IterableByKeyMap`] for such type automatically.
pub trait AuxiliaryDoubleStorageWrap {
    type Key1: Ord + Clone;
    type Key2: Ord + Clone;
    type Value: Clone;
    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R;

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R;
}

impl<T: AuxiliaryDoubleStorageWrap> DoubleMapStorage for T {
    type Key1 = T::Key1;
    type Key2 = T::Key2;
    type Value = T::Value;

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
        T::with_storage(|map| map.get(key1, key2).cloned())
    }

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value) {
        T::with_storage_mut(|map| map.insert(key1, key2, value));
    }

    fn clear() {
        T::with_storage_mut(|map| map.clear());
    }

    fn clear_prefix(first_key: Self::Key1) {
        T::with_storage_mut(|map| {
            let keys = map.iter_key(&first_key).map(|(k, _)| k.clone());
            for key in keys {
                map.remove(first_key.clone(), key);
            }
        });
    }

    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
        T::with_storage_mut(|map| map.contains_keys(key1, key2))
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> R {
        T::with_storage_mut(|map| {
            let inner_map = map.inner.entry(key1).or_default();
            match inner_map.entry(key2) {
                Entry::Occupied(mut occupied) => {
                    let mut value = Some(occupied.get().clone());
                    let result = f(&mut value);
                    if let Some(value) = value {
                        *occupied.get_mut() = value;
                    } else {
                        occupied.remove();
                    }

                    result
                }

                Entry::Vacant(vacant) => {
                    let mut value = None;
                    let result = f(&mut value);
                    if let Some(value) = value {
                        vacant.insert(value);
                    }
                    result
                }
            }
        })
    }

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> Option<R> {
        T::with_storage_mut(|map| {
            if let Some(inner_map) = map.inner.get_mut(&key1)
                && let Some(value) = inner_map.get_mut(&key2)
            {
                return Some(f(value));
            }

            None
        })
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
        T::with_storage_mut(|map| {
            for (_, inner_map) in map.inner.iter_mut() {
                for (_, value) in inner_map.iter_mut() {
                    *value = f(value.clone());
                }
            }
        });
    }

    fn remove(key1: Self::Key1, key2: Self::Key2) {
        Self::take(key1, key2);
    }

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
        T::with_storage_mut(|map| map.remove(key1, key2))
    }
}

impl<T: AuxiliaryDoubleStorageWrap> IterableByKeyMap<T::Value> for T {
    type Key = T::Key1;

    type DrainIter = IteratorWrap<IntoIter<T::Key2, T::Value>, T::Value, GetSecondPos>;

    type Iter = IteratorWrap<IntoIter<T::Key2, T::Value>, T::Value, GetSecondPos>;

    fn drain_key(key: Self::Key) -> Self::DrainIter {
        T::with_storage_mut(|map| map.drain_key(&key)).into()
    }

    fn iter_key(key: Self::Key) -> Self::Iter {
        T::with_storage(|map| map.iter_key(&key)).into()
    }
}

impl<T: AuxiliaryDoubleStorageWrap> KeyIterableByKeyMap for T {
    type Key1 = T::Key1;
    type Key2 = T::Key2;
    type DrainIter = IteratorWrap<IntoIter<T::Key2, T::Value>, T::Key2, GetFirstPos>;
    type Iter = IteratorWrap<IntoIter<T::Key2, T::Value>, T::Key2, GetFirstPos>;

    fn drain_prefix_keys(key: Self::Key1) -> Self::DrainIter {
        T::with_storage_mut(|map| map.drain_key(&key).into())
    }

    fn iter_prefix_keys(key: Self::Key1) -> Self::Iter {
        T::with_storage(|map| map.iter_key(&key)).into()
    }
}

impl<T: AuxiliaryDoubleStorageWrap> CountedByKey for T {
    type Key = T::Key1;
    type Length = usize;

    fn len(key: &Self::Key) -> Self::Length {
        T::with_storage(|map| map.count_key(key))
    }
}

pub trait AuxiliaryStorageWrap {
    type Key: Clone + Ord;
    type Value: Clone;

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&BTreeMap<Self::Key, Self::Value>) -> R;

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut BTreeMap<Self::Key, Self::Value>) -> R;
}

impl<T: AuxiliaryStorageWrap> MapStorage for T {
    type Key = T::Key;
    type Value = T::Value;

    fn clear() {
        T::with_storage_mut(|map| map.clear());
    }

    fn contains_key(key: &Self::Key) -> bool {
        T::with_storage(|map| map.contains_key(key))
    }

    fn get(key: &Self::Key) -> Option<Self::Value> {
        T::with_storage(|map| map.get(key).cloned())
    }

    fn insert(key: Self::Key, value: Self::Value) {
        T::with_storage_mut(|map| map.insert(key, value));
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R {
        T::with_storage_mut(|map| match map.entry(key) {
            Entry::Occupied(mut occupied) => {
                let mut value = Some(occupied.get().clone());

                let result = f(&mut value);
                if let Some(value) = value.take() {
                    *occupied.get_mut() = value;
                } else {
                    occupied.remove();
                }

                result
            }

            Entry::Vacant(vacant) => {
                let mut value = None;

                let result = f(&mut value);

                if let Some(value) = value.take() {
                    vacant.insert(value);
                }

                result
            }
        })
    }

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(key: Self::Key, f: F) -> Option<R> {
        T::with_storage_mut(|map| map.get_mut(&key).map(f))
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
        T::with_storage_mut(|map| {
            map.iter_mut()
                .for_each(|(_, value)| *value = f(value.clone()))
        });
    }

    fn remove(key: Self::Key) {
        Self::take(key);
    }

    fn take(key: Self::Key) -> Option<Self::Value> {
        T::with_storage_mut(|map| map.remove(&key))
    }
}

impl<T: AuxiliaryStorageWrap> Counted for T {
    type Length = usize;
    fn len() -> Self::Length {
        T::with_storage(|map| map.len())
    }
}
