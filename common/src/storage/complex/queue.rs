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
    complicated::{LinkedList, LinkedListError},
    primitives::{Counted, IterableMap, KeyFor},
};
use core::marker::PhantomData;

pub trait Queue {
    type Value;
    type Error: LinkedListError;
    type OutputError: From<Self::Error>;

    fn dequeue() -> Result<Option<Self::Value>, Self::OutputError>;

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    fn remove_all();

    fn requeue(value: Self::Value) -> Result<(), Self::OutputError>;

    fn queue(value: Self::Value) -> Result<(), Self::OutputError>;
}

pub struct QueueImpl<T, OutputError, KeyGen>(PhantomData<(T, OutputError, KeyGen)>)
where
    T: LinkedList,
    OutputError: From<T::Error>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>;

impl<T, OutputError, KeyGen> Queue for QueueImpl<T, OutputError, KeyGen>
where
    T: LinkedList,
    OutputError: From<T::Error>,
    T::Error: LinkedListError,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type Value = T::Value;
    type Error = T::Error;
    type OutputError = OutputError;

    fn dequeue() -> Result<Option<Self::Value>, Self::OutputError> {
        T::pop_front().map_err(Into::into)
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F) {
        T::mutate_values(f)
    }

    fn remove_all() {
        T::remove_all()
    }

    fn requeue(value: Self::Value) -> Result<(), Self::OutputError> {
        let key = KeyGen::key_for(&value);
        T::push_front(key, value).map_err(Into::into)
    }

    fn queue(value: Self::Value) -> Result<(), Self::OutputError> {
        let key = KeyGen::key_for(&value);
        T::push_back(key, value).map_err(Into::into)
    }
}

impl<T, OutputError, KeyGen> Counted for QueueImpl<T, OutputError, KeyGen>
where
    T: LinkedList + Counted,
    OutputError: From<T::Error>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type Length = T::Length;

    fn len() -> Self::Length {
        T::len()
    }
}

impl<T, OutputError, KeyGen> IterableMap<Result<T::Value, OutputError>>
    for QueueImpl<T, OutputError, KeyGen>
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type DrainIter = QueueDrainIter<T, OutputError>;
    type Iter = QueueIter<T, OutputError>;

    fn drain() -> Self::DrainIter {
        QueueDrainIter(T::drain(), PhantomData::<OutputError>)
    }

    fn iter() -> Self::Iter {
        QueueIter(T::iter(), PhantomData::<OutputError>)
    }
}

pub struct QueueDrainIter<T, OutputError>(T::DrainIter, PhantomData<OutputError>)
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>;

impl<T, OutputError> Iterator for QueueDrainIter<T, OutputError>
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>,
{
    type Item = Result<T::Value, OutputError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|res| res.map_err(Into::into))
    }
}

pub struct QueueIter<T, OutputError>(T::Iter, PhantomData<OutputError>)
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>;

impl<T, OutputError> Iterator for QueueIter<T, OutputError>
where
    T: LinkedList + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>,
{
    type Item = Result<T::Value, OutputError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|res| res.map_err(Into::into))
    }
}
