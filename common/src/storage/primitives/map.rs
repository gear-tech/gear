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

pub trait MapStorage {
    type Key;
    type Value;

    fn contains_key(key: &Self::Key) -> bool;

    fn get(key: &Self::Key) -> Option<Self::Value>;

    fn insert(key: Self::Key, value: Self::Value);

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(key: Self::Key, f: F) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(key: Self::Key, f: F) -> Option<R> {
        Self::mutate(key, |opt_val| opt_val.as_mut().map(f))
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    fn remove(key: Self::Key);

    fn remove_all();

    fn take(key: Self::Key) -> Option<Self::Value>;
}

#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_map {
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty) => {
        wrap_storage_map!(storage: $storage, name: $name, key: $key, value: $val, counted None);
    };
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty, counted $($rm_arg:expr)?) => {
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

            fn remove_all() {
                $storage::<T>::remove_all($($rm_arg)?);
            }

            fn take(key: Self::Key) -> Option<Self::Value> {
                $storage::<T>::take(key)
            }
        }
    };
}

#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_counted_storage_map {
    (storage: $storage: ident, name: $name: ident, key: $key: ty, value: $val: ty, length: $len: ty) => {
        $crate::wrap_storage_map!(
            storage: $storage,
            name: $name,
            key: $key,
            value: $val,
            counted
        );

        impl<T: crate::Config> Counted for $name<T> {
            type Length = $len;

            fn len() -> Self::Length {
                $storage::<T>::count() as Self::Length
            }
        }
    };
}
