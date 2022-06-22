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

//! Module for single-value storing primitive.
//!
//! This primitive defines interface of interaction
//! with globally stored single-value.

/// Represents logic of managing globally stored
/// value for more complicated logic.
///
/// In fact, represents custom implementation/wrapper
/// around of Substrate's `ValueStorage` with `OptionQuery`.
pub trait ValueStorage {
    /// Stored value type.
    type Value;

    /// Returns bool, defining does value present.
    fn exists() -> bool;

    /// Gets stored value, if present.
    fn get() -> Option<Self::Value>;

    /// Removes stored value.
    fn kill();

    /// Mutates stored value by `Option` reference, which stored
    /// (or not in `None` case) with given function.
    ///
    /// May return generic type value.
    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(f: F) -> R;

    /// Works the same as `Self::mutate`, but triggers if value present.
    fn mutate_exists<R, F: FnOnce(&mut Self::Value) -> R>(f: F) -> Option<R> {
        Self::mutate(|opt_val| opt_val.as_mut().map(f))
    }

    /// Stores given value.
    fn put(value: Self::Value);

    /// Stores given value and returns previous one, if present.
    fn set(value: Self::Value) -> Option<Self::Value>;

    /// Gets stored value, if present, and removes it from storage.
    fn take() -> Option<Self::Value>;
}

/// Creates new type with specified name and value type and implements
/// `ValueStorage` for it based on specified storage,
/// which is a `Substrate`'s `StorageValue`.
///
/// This macro main purpose is to follow newtype pattern
/// and avoid `Substrate` dependencies in `gear_common`.
///
/// Requires `PhantomData` be in scope: from `std`, `core` or `sp_std`.
///
/// Requires `Config` be in scope of the crate root where it called.
#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! wrap_storage_value {
    (storage: $storage: ident, name: $name: ident, value: $val: ty) => {
        #[derive(Debug, PartialEq, Eq)]
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
