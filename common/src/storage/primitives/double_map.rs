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

    fn elements_with(key1: &Self::Key1) -> usize;

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

    fn mutate_values<F: FnOnce(Self::Value) -> Self::Value>(f: F);

    fn remove(key1: Self::Key1, key2: Self::Key2);

    fn remove_all() -> Result<(), u8>;

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value>;
}
