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

    /// Returns `Vec` of values, which share the given first key.
    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value>;

    /// Returns bool, defining does map contain value under given keys.
    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    /// Returns amount of second keys and values under given first key.
    fn count_of(key: &Self::Key1) -> usize;

    /// Gets value stored under given keys, if present.
    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value>;

    /// Inserts value with given keys.
    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value);

    /// Mutates value by `Option` reference, which stored
    /// (or not in `None` case) under given keys
    /// with given function.
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
    fn remove_all();

    /// Gets value stored under given keys, if present,
    /// and removes it from storage.
    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value>;
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
#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_double_map {
    (storage: $storage: ident, name: $name: ident, key1: $key1: ty, key2: $key2: ty, value: $val: ty) => {
        pub struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> DoubleMapStorage for MailboxWrap<T> {
            type Key1 = $key1;
            type Key2 = $key2;
            type Value = $val;

            fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
                $storage::<T>::contains_key(key1, key2)
            }

            fn collect_of(key: Self::Key1) -> Vec<Self::Value> {
                $storage::<T>::iter_prefix_values(key).collect()
            }

            fn count_of(key: &Self::Key1) -> usize {
                $storage::<T>::iter_key_prefix(key).count()
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

            fn remove_all() {
                $storage::<T>::remove_all(None);
            }

            fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
                $storage::<T>::take(key1, key2)
            }
        }
    };
}
