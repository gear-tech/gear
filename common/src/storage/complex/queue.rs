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

use crate::storage::{
    complicated::LinkedList,
    primitives::{Counted, IterableMap, KeyFor},
};
use core::marker::PhantomData;

pub trait Queue {
    type Value;
    type Error;

    fn dequeue() -> Result<Option<Self::Value>, Self::Error>;

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    fn remove_all();

    fn requeue(value: Self::Value) -> Result<(), Self::Error>;

    fn queue(value: Self::Value) -> Result<(), Self::Error>;
}

pub struct QueueImpl<T: LinkedList, KeyGen: KeyFor<Key = T::Key, Value = T::Value>>(
    PhantomData<(T, KeyGen)>,
);

impl<T: LinkedList, KeyGen: KeyFor<Key = T::Key, Value = T::Value>> Queue for QueueImpl<T, KeyGen> {
    type Value = T::Value;
    type Error = T::Error;

    fn dequeue() -> Result<Option<Self::Value>, Self::Error> {
        T::pop_front()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F) {
        T::mutate_values(f)
    }

    fn remove_all() {
        T::remove_all()
    }

    fn requeue(value: Self::Value) -> Result<(), Self::Error> {
        let key = KeyGen::key_for(&value);
        T::push_front(key, value)
    }

    fn queue(value: Self::Value) -> Result<(), Self::Error> {
        let key = KeyGen::key_for(&value);
        T::push_back(key, value)
    }
}

impl<T, KeyGen> Counted for QueueImpl<T, KeyGen>
where
    T: LinkedList + Counted,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type Length = T::Length;

    fn len() -> Self::Length {
        T::len()
    }
}

impl<T, KeyGen> IterableMap<Result<T::Value, T::Error>> for QueueImpl<T, KeyGen>
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type DrainIter = T::DrainIter;
    type Iter = T::Iter;

    fn drain() -> Self::DrainIter {
        T::drain()
    }

    fn iter() -> Self::Iter {
        T::iter()
    }
}
