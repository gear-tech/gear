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

pub trait DoubleMapStorage {
    type Key1;
    type Key2;
    type Value;

    fn contains_key(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value>;

    fn count_of(key: &Self::Key1) -> usize;

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value>;

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value);

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(
        key1: Self::Key1,
        key2: Self::Key2,
        f: F,
    ) -> Option<R> {
        Self::mutate(key1, key2, |opt_val| opt_val.as_mut().map(f))
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    fn remove(key1: Self::Key1, key2: Self::Key2);

    fn remove_all();

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value>;
}

#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_double_map {
    (storage: $storage: ident, name: $name: ident, key1: $key1: ty, key2: $key2: ty, value: $val: ty) => {
        pub struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> DoubleMapStorage for MailboxWrap<T> {
            type Key1 = $key1;
            type Key2 = $key2;
            type Value = $val;

            fn contains_key(key1: &Self::Key1, key2: &Self::Key2) -> bool {
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
