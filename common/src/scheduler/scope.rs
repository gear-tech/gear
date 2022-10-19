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

use crate::storage::{CountedByKey, DoubleMapStorage, EmptyCallback, KeyIterableByKeyMap};
use core::marker::PhantomData;

/// Represents tasks managing logic.
pub trait TaskPool {
    /// Block number type.
    type BlockNumber;
    /// Task type.
    type Task;
    /// Inner error type of queue storing algorithm.
    type Error: TaskPoolError;
    /// Output error type of the queue.
    type OutputError: From<Self::Error>;

    /// Inserts given task in task pool.
    fn add(bn: Self::BlockNumber, task: Self::Task) -> Result<(), Self::OutputError>;

    /// Removes all tasks from task pool.
    fn clear();

    /// Returns bool, defining does task exist in task pool.
    fn contains(bn: &Self::BlockNumber, task: &Self::Task) -> bool;

    /// Removes task from task pool by given keys,
    /// if present, else returns error.
    fn delete(bn: Self::BlockNumber, task: Self::Task) -> Result<(), Self::OutputError>;
}

/// Represents store of task pool's action callbacks.
pub trait TaskPoolCallbacks {
    /// Callback on success `add`.
    type OnAdd: EmptyCallback;
    /// Callback on success `delete`.
    type OnDelete: EmptyCallback;
}

/// Represents task pool error type.
///
/// Contains constructors for all existing errors.
pub trait TaskPoolError {
    /// Occurs when given task already exists in task pool.
    fn duplicate_task() -> Self;

    /// Occurs when task wasn't found in storage.
    fn task_not_found() -> Self;
}

/// `TaskPool` implementation based on `DoubleMapStorage`.
///
/// Generic parameter `Error` requires `TaskPoolError` implementation.
/// Generic parameter `Callbacks` presents actions for success operations
/// over task pool.
pub struct TaskPoolImpl<T, Task, Error, OutputError, Callbacks>(
    PhantomData<(T, Task, Error, OutputError, Callbacks)>,
)
where
    T: DoubleMapStorage<Key2 = Task, Value = ()>,
    Error: TaskPoolError,
    OutputError: From<Error>,
    Callbacks: TaskPoolCallbacks;

// Implementation of `TaskPool` for `TaskPoolImpl`.
impl<T, Task, Error, OutputError, Callbacks> TaskPool
    for TaskPoolImpl<T, Task, Error, OutputError, Callbacks>
where
    T: DoubleMapStorage<Key2 = Task, Value = ()>,
    Error: TaskPoolError,
    OutputError: From<Error>,
    Callbacks: TaskPoolCallbacks,
{
    type BlockNumber = T::Key1;
    type Task = T::Key2;
    type Error = Error;
    type OutputError = OutputError;

    fn add(bn: Self::BlockNumber, task: Self::Task) -> Result<(), Self::OutputError> {
        if !Self::contains(&bn, &task) {
            T::insert(bn, task, ());
            Callbacks::OnAdd::call();
            Ok(())
        } else {
            Err(Self::Error::duplicate_task().into())
        }
    }

    fn clear() {
        T::clear()
    }

    fn contains(bn: &Self::BlockNumber, task: &Self::Task) -> bool {
        T::contains_keys(bn, task)
    }

    fn delete(bn: Self::BlockNumber, task: Self::Task) -> Result<(), Self::OutputError> {
        if T::contains_keys(&bn, &task) {
            T::remove(bn, task);
            Callbacks::OnDelete::call();
            Ok(())
        } else {
            Err(Self::Error::task_not_found().into())
        }
    }
}

// Implementation of `CountedByKey` trait for `TaskPoolImpl` in case,
// when inner `DoubleMapStorage` implements `CountedByKey`.
impl<T, Task, Error, OutputError, Callbacks> CountedByKey
    for TaskPoolImpl<T, Task, Error, OutputError, Callbacks>
where
    T: DoubleMapStorage<Key2 = Task, Value = ()> + CountedByKey<Key = T::Key1>,
    Error: TaskPoolError,
    OutputError: From<Error>,
    Callbacks: TaskPoolCallbacks,
{
    type Key = T::Key1;
    type Length = T::Length;

    fn len(key: &Self::Key) -> Self::Length {
        T::len(key)
    }
}

// Implementation of `KeyIterableByKeyMap` trait for `TaskPoolImpl` in case,
// when inner `DoubleMapStorage` implements `KeyIterableByKeyMap`.
impl<T, Task, Error, OutputError, Callbacks> KeyIterableByKeyMap
    for TaskPoolImpl<T, Task, Error, OutputError, Callbacks>
where
    T: DoubleMapStorage<Key2 = Task, Value = ()> + KeyIterableByKeyMap,
    Error: TaskPoolError,
    OutputError: From<Error>,
    Callbacks: TaskPoolCallbacks,
{
    type Key1 = <T as KeyIterableByKeyMap>::Key1;
    type Key2 = <T as KeyIterableByKeyMap>::Key2;
    type DrainIter = T::DrainIter;
    type Iter = T::Iter;

    fn drain_prefix_keys(bn: Self::Key1) -> Self::DrainIter {
        T::drain_prefix_keys(bn)
    }

    fn iter_prefix_keys(bn: Self::Key1) -> Self::Iter {
        T::iter_prefix_keys(bn)
    }
}
