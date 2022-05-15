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

pub trait ValueStorage {
    type Value;

    fn exists() -> bool;

    fn get() -> Option<Self::Value>;

    fn kill();

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R;

    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(f: F) -> Option<R> {
        Self::mutate(|opt_val| opt_val.as_mut().map(f))
    }

    fn put(value: Self::Value);

    fn set(value: Self::Value) -> Option<Self::Value>;

    fn take() -> Option<Self::Value>;
}

#[allow(unknown_lints, clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_value {
    (storage: $storage: ident, name: $name: ident, value: $val: ty) => {
        pub struct $name<T>(PhantomData<T>);

        impl<T: crate::Config> ValueStorage for $name<T> {
            type Value = $val;

            fn exists() -> bool {
                $storage::<T>::exists()
            }

            fn get() -> Option<Self::Value> {
                $storage::<T>::get()
            }

            fn kill() {
                $storage::<T>::kill()
            }

            fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R {
                $storage::<T>::mutate(f)
            }

            fn put(value: Self::Value) {
                $storage::<T>::put(value)
            }

            fn set(value: Self::Value) -> Option<Self::Value> {
                $storage::<T>::mutate(|opt| {
                    let prev = opt.take();
                    *opt = Some(value);
                    prev
                })
            }

            fn take() -> Option<Self::Value> {
                $storage::<T>::take()
            }
        }
    };
}
