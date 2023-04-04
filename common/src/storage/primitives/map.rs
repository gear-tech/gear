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

//! Module for map storing primitive.
//!
//! This primitive defines interface of interaction
//! with globally stored single-key map (Key -> Value).

use frame_support::codec::{Encode, EncodeAppend, EncodeLike};

/// Represents logic of managing globally stored
/// single-key map for more complicated logic.
///
/// In fact, represents custom implementation/wrapper
/// around of Substrate's `StorageMap` with `OptionQuery`.
pub trait MapStorage {
    /// Map's key type.
    type Key;
    /// Map's stored value type.
    type Value;

    /// Returns bool, defining does map contain value under given key.
    fn contains_key(key: &Self::Key) -> bool;

    /// Gets value stored under given key, if present.
    fn get(key: &Self::Key) -> Option<Self::Value>;

    /// Inserts value with given key.
    fn insert(key: Self::Key, value: Self::Value);

    /// Mutates value by `Option` reference, which stored (or not
    /// in `None` case) under given key with given function.
    ///
    /// May return generic type value.
    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R;

    /// Works the same as `Self::mutate`, but triggers if value present.
    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(key: Self::Key, f: F) -> Option<R> {
        Self::mutate(key, |opt_val| opt_val.as_mut().map(f))
    }

    /// Mutates all stored values with given convert function.
    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    /// Removes value stored under the given key.
    fn remove(key: Self::Key);

    /// Removes all values.
    fn clear();

    /// Gets value stored under given key, if present,
    /// and removes it from storage.
    fn take(key: Self::Key) -> Option<Self::Value>;
}

pub trait AppendMapStorage<Item, Key, Value>: MapStorage<Key = Key, Value = Value>
where
    Item: Encode,
    Key: Encode,
    Value: EncodeAppend<Item = Item>,
{
    fn append<EncodeLikeKey, EncodeLikeItem>(key: EncodeLikeKey, item: EncodeLikeItem)
    where
        EncodeLikeKey: EncodeLike<Key>,
        EncodeLikeItem: EncodeLike<Item>;
}

/// Creates new type with specified name and key-value types and implements
/// `MapStorage` for it based on specified storage,
/// which is a `Substrate`'s `StorageMap`.
///
/// This macro main purpose is to follow newtype pattern
/// and avoid `Substrate` dependencies in `gear_common`.
///
/// Requires `PhantomData` be in scope: from `std`, `core` or `sp_std`.
///
/// Requires `Config` be in scope of the crate root where it called.
///
/// Has two implementations to provide auto addition of `Counted` logic
/// (for `Substrate`'s `CountedStorageMap`) due to storage's
/// arguments difference.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_map {
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty) => {
        pub struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> MapStorage for $name<T> {
            type Key = $key;
            type Value = $val;

            fn contains_key(key: &Self::Key) -> bool {
                $storage::<T>::contains_key(key)
            }

            fn get(key: &Self::Key) -> Option<Self::Value> {
                $storage::<T>::get(key)
            }

            fn insert(key: Self::Key, value: Self::Value) {
                $storage::<T>::insert(key, value)
            }

            fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R {
                $storage::<T>::mutate(key, f)
            }

            fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
                let f = |v| Some(f(v));
                $storage::<T>::translate_values(f)
            }

            fn remove(key: Self::Key) {
                $storage::<T>::remove(key)
            }

            fn clear() {
                let _ = $storage::<T>::clear(u32::MAX, None);
            }

            fn take(key: Self::Key) -> Option<Self::Value> {
                $storage::<T>::take(key)
            }
        }
    };
}

/// Same as `wrap_storage_map!`, but with length type parameter
/// to auto-impl `Counted` trait of `gear_common` storage primitives.
///
/// Better to use Rust's numeric types as `Length`.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_counted_storage_map {
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty, length: $len: ty) => {
        $crate::wrap_storage_map!(storage: $storage, name: $name, key: $key, value: $val);

        impl<T: crate::Config> Counted for $name<T> {
            type Length = $len;

            fn len() -> Self::Length {
                $storage::<T>::count() as Self::Length
            }
        }
    };
}
