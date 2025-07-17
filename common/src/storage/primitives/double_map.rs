// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

//! Module for double map storing primitive.
//!
//! This primitive defines interface of interaction
//! with globally stored double-key map (Key1 -> Key2 -> Value).

/// Represents logic of managing globally stored
/// double-key map for more complicated logic.
///
/// In fact, represents custom implementation/wrapper
/// around of Substrate's `StorageDoubleMap` with `OptionQuery`.
pub trait DoubleMapStorage {
    /// Map's first key type.
    type Key1;
    /// Map's second key type.
    type Key2;
    /// Map's stored value type.
    type Value;

    /// Returns bool, defining does map contain value under given keys.
    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    /// Gets value stored under given keys, if present.
    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value>;

    /// Inserts value with given keys.
    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value);

    /// Mutates value by `Option` reference, which stored (or not
    /// in `None` case) under given keys with given function.
    ///
    /// May return generic type value.
    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> R;

    /// Works the same as `Self::mutate`, but triggers if value present.
    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> Option<R> {
        Self::mutate(key1, key2, |opt_val| opt_val.as_mut().map(f))
    }

    /// Mutates all stored values with given convert function.
    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    /// Removes value stored under the given keys.
    fn remove(key1: Self::Key1, key2: Self::Key2);

    /// Removes all values.
    fn clear();

    /// Gets value stored under given keys, if present,
    /// and removes it from storage.
    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value>;

    /// Remove items from the map matching a `first_key` prefix.
    fn clear_prefix(first_key: Self::Key1);
}

/// Creates new type with specified name and key1-key2-value types and
/// implements `DoubleMapStorage` for it based on specified storage,
/// which is a `Substrate`'s `StorageDoubleMap`.
///
/// This macro main purpose is to follow newtype pattern
/// and avoid `Substrate` dependencies in `gear_common`.
///
/// Requires `PhantomData` be in scope: from `std`, `core` or `sp_std`.
///
/// Requires `Config` be in scope of the crate root where it called.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_double_map {
    (storage: $storage: ident, name: $name: ident, key1: $key1: ty,
        key2: $key2: ty, value: $val: ty) => {
        pub struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> DoubleMapStorage for $name<T> {
            type Key1 = $key1;
            type Key2 = $key2;
            type Value = $val;

            fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
                $storage::<T>::contains_key(key1, key2)
            }

            fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
                $storage::<T>::get(key1, key2)
            }

            fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value) {
                $storage::<T>::insert(key1, key2, value)
            }

            fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
                key1: Self::Key1,
                key2: Self::Key2,
                f: F,
            ) -> R {
                $storage::<T>::mutate(key1, key2, f)
            }

            fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
                let f = |v| Some(f(v));
                $storage::<T>::translate_values(f)
            }

            fn remove(key1: Self::Key1, key2: Self::Key2) {
                $storage::<T>::remove(key1, key2)
            }

            fn clear() {
                let _ = $storage::<T>::clear(u32::MAX, None);
            }

            fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
                $storage::<T>::take(key1, key2)
            }

            fn clear_prefix(first_key: Self::Key1) {
                let _ = $storage::<T>::clear_prefix(first_key, u32::MAX, None);
            }
        }
    };
}

/// Same as `wrap_storage_double_map!`, but with extra implementations
/// of `CountedByKey`, `IterableMap` and `IterableByKeyMap`
/// over double map values.
///
/// `PrefixIterator` from `frame_support` and `KeyValueIteratorWrap` from
/// this crate should be in scope.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_extended_storage_double_map {
    (storage: $storage: ident, name: $name: ident, key1: $key1: ty,
        key2: $key2: ty, value: $val: ty, length: $len: ty) => {
        $crate::wrap_storage_double_map!(
            storage: $storage,
            name: $name,
            key1: $key1,
            key2: $key2,
            value: $val
        );

        impl<T: crate::Config> CountedByKey for $name<T> {
            type Key = $key1;
            type Length = $len;

            fn len(key: &Self::Key) -> Self::Length {
                $storage::<T>::iter_prefix(key).count()
            }
        }

        impl<T: crate::Config> IterableByKeyMap<$val> for $name<T> {
            type Key = $key1;
            type DrainIter = IteratorWrap<PrefixIterator<($key2, $val)>, $val, GetSecondPos>;
            type Iter = IteratorWrap<PrefixIterator<($key2, $val)>, $val, GetSecondPos>;

            fn drain_key(key: Self::Key) -> Self::DrainIter {
                $storage::<T>::drain_prefix(key).into()
            }

            fn iter_key(key: Self::Key) -> Self::Iter {
                $storage::<T>::iter_prefix(key).into()
            }
        }

        impl<T: crate::Config> IterableMap<$val> for $name<T> {
            type DrainIter = IteratorWrap<PrefixIterator<($key1, $key2, $val)>, $val, GetThirdPos>;
            type Iter = IteratorWrap<PrefixIterator<($key1, $key2, $val)>, $val, GetThirdPos>;

            fn drain() -> Self::DrainIter {
                $storage::<T>::drain().into()
            }

            fn iter() -> Self::Iter {
                $storage::<T>::iter().into()
            }
        }

        impl<T: crate::Config> KeyIterableByKeyMap for $name<T> {
            type Key1 = $key1;
            type Key2 = $key2;
            type DrainIter = IteratorWrap<PrefixIterator<($key2, $val)>, $key2, GetFirstPos>;
            type Iter = IteratorWrap<PrefixIterator<($key2, $val)>, $key2, GetFirstPos>;

            fn drain_prefix_keys(key: Self::Key1) -> Self::DrainIter {
                $storage::<T>::drain_prefix(key).into()
            }

            fn iter_prefix_keys(key: Self::Key1) -> Self::Iter {
                $storage::<T>::iter_prefix(key).into()
            }
        }
    };
}

#[cfg(feature = "std")]
pub mod auxiliary_double_map {
    use crate::storage::{
        Counted, CountedByKey, DoubleMapStorage, GetFirstPos, GetSecondPos, IterableByKeyMap,
        IteratorWrap, KeyIterableByKeyMap, MapStorage,
    };
    use std::collections::btree_map::{BTreeMap, Entry, IntoIter};

    /// Double key `BTreeMap`.
    ///
    /// Basically is just a map of the map.
    #[derive(Clone)]
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
}
