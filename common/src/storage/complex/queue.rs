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

//! Module for message queue implementation.
//!
//! Message queue provides functionality of storing messages,
//! addressed to programs.

use crate::storage::{Counted, Dequeue, DequeueError, IterableMap, KeyFor};
use core::marker::PhantomData;

/// Represents message queue managing logic.
pub trait Queue {
    /// Stored values type.
    type Value;
    /// Inner error type of queue storing algorithm.
    type Error: DequeueError;
    /// Output error type of the queue.
    type OutputError: From<Self::Error>;

    /// Removes and returns message from the beginning of the queue,
    /// if present,
    fn dequeue() -> Result<Option<Self::Value>, Self::OutputError>;

    /// Mutates all values in queue with given function.
    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    /// Inserts given value at the end of the queue.
    fn queue(value: Self::Value) -> Result<(), Self::OutputError>;

    /// Removes all values from queue.
    fn clear();

    /// Inserts given value at the beginning of the queue.
    ///
    /// Should be used only for cases, when message was dequeued and
    /// it's execution should be postponed until the next block.
    fn requeue(value: Self::Value) -> Result<(), Self::OutputError>;
}

/// `Mailbox` implementation based on `Dequeue`.
///
/// Generic parameter `KeyGen` presents key generation for given values.
pub struct QueueImpl<T, OutputError, KeyGen>(PhantomData<(T, OutputError, KeyGen)>)
where
    T: Dequeue,
    OutputError: From<T::Error>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>;

// Implementation of `Queue` for `QueueImpl`.
impl<T, OutputError, KeyGen> Queue for QueueImpl<T, OutputError, KeyGen>
where
    T: Dequeue,
    OutputError: From<T::Error>,
    T::Error: DequeueError,
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

    fn queue(value: Self::Value) -> Result<(), Self::OutputError> {
        let key = KeyGen::key_for(&value);
        T::push_back(key, value).map_err(Into::into)
    }

    fn clear() {
        T::clear()
    }

    fn requeue(value: Self::Value) -> Result<(), Self::OutputError> {
        let key = KeyGen::key_for(&value);
        T::push_front(key, value).map_err(Into::into)
    }
}

// Implementation of `Counted` trait for `QueueImpl` in case,
// when inner `Dequeue` implements `Counted.
impl<T, OutputError, KeyGen> Counted for QueueImpl<T, OutputError, KeyGen>
where
    T: Dequeue + Counted,
    OutputError: From<T::Error>,
    KeyGen: KeyFor<Key = T::Key, Value = T::Value>,
{
    type Length = T::Length;

    fn len() -> Self::Length {
        T::len()
    }
}

/// Drain iterator over queue's values.
///
/// Removes element on each iteration.
pub struct QueueDrainIter<T, OutputError>(T::DrainIter, PhantomData<OutputError>)
where
    T: Dequeue + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>;

// `Iterator` implementation for `QueueDrainIter`.
impl<T, OutputError> Iterator for QueueDrainIter<T, OutputError>
where
    T: Dequeue + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>,
{
    type Item = Result<T::Value, OutputError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|res| res.map_err(Into::into))
    }
}

/// Common iterator over queue's values.
pub struct QueueIter<T, OutputError>(T::Iter, PhantomData<OutputError>)
where
    T: Dequeue + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>;

// `Iterator` implementation for `QueueIter`.
impl<T, OutputError> Iterator for QueueIter<T, OutputError>
where
    T: Dequeue + IterableMap<Result<T::Value, T::Error>>,
    OutputError: From<T::Error>,
{
    type Item = Result<T::Value, OutputError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|res| res.map_err(Into::into))
    }
}

// `IterableMap` implementation for `QueueImpl`, returning iterators,
// presented with `QueueIter` and `QueueDrainIter`.
impl<T, OutputError, KeyGen> IterableMap<Result<T::Value, OutputError>>
    for QueueImpl<T, OutputError, KeyGen>
where
    T: Dequeue + IterableMap<Result<T::Value, T::Error>>,
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
