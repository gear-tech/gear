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
