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

//! Module for dequeue implementation.
//!
//! Dequeue based on dequeue implementation over key value map.
//! This dequeue algorithm has main invariants:
//! - If dequeue is empty, it's head and tail should be empty.
//! - If dequeue contains the only one elements, is'ts head and tail
//! should equal this element's key.
//! - Based on above specified points, head and tail should
//! both be set or be empty.
//! - Inner map should contain values under keys, set in head and tail,
//! if they present.

use crate::storage::{Callback, Counted, EmptyCallback, IterableMap, MapStorage, ValueStorage};
use core::marker::PhantomData;
use frame_support::{
    codec::{self, Decode, Encode},
    scale_info::{self, TypeInfo},
};

/// Represents dequeue implementation.
pub trait Dequeue {
    /// Dequeue's elements stored key.
    type Key;
    /// Dequeue's elements stored value.
    type Value;
    /// Dequeue error type.
    type Error;

    /// Mutates all stored value with given function.
    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    /// Removes and returns tail value of the dequeue, if present.
    fn pop_back() -> Result<Option<Self::Value>, Self::Error>;

    /// Removes and returns head value of the dequeue, if present.
    fn pop_front() -> Result<Option<Self::Value>, Self::Error>;

    /// Inserts value to the end of dequeue with given key.
    fn push_back(key: Self::Key, value: Self::Value) -> Result<(), Self::Error>;

    /// Inserts value to the beginning of dequeue with given key.
    fn push_front(key: Self::Key, value: Self::Value) -> Result<(), Self::Error>;

    /// Removes all values.
    fn clear();
}

/// Represents store of dequeue's action callbacks.
pub trait DequeueCallbacks {
    /// Callback relative type.
    ///
    /// This value should be the main item of dequeue,
    /// which uses this callbacks store.
    type Value;

    /// Callback on success `pop_back`.
    type OnPopBack: Callback<Self::Value>;
    /// Callback on success `pop_front`.
    type OnPopFront: Callback<Self::Value>;
    /// Callback on success `push_back`.
    type OnPushBack: Callback<Self::Value>;
    /// Callback on success `push_front`.
    type OnPushFront: Callback<Self::Value>;
    /// Callback on success `clear`.
    type OnClear: EmptyCallback;
}

/// Represents dequeue error type.
///
/// Contains constructors for all existing errors.
pub trait DequeueError {
    /// Occurs when given key already exists in dequeue.
    fn duplicate_key() -> Self;

    /// Occurs when element wasn't found in storage.
    fn element_not_found() -> Self;

    /// Occurs when head should contain value,
    /// but it's empty for some reason.
    fn head_should_be_set() -> Self;

    /// Occurs when head should be empty,
    /// but it contains value for some reason.
    fn head_should_not_be_set() -> Self;

    /// Occurs when tail element of the dequeue
    /// contains link to the next element.
    fn tail_has_next_key() -> Self;

    /// Occurs when while searching pre-tail,
    /// element wasn't found.
    fn tail_parent_not_found() -> Self;

    /// Occurs when tail should contain value,
    /// but it's empty for some reason.
    fn tail_should_be_set() -> Self;

    /// Occurs when tail should be empty,
    /// but it contains value for some reason.
    fn tail_should_not_be_set() -> Self;
}

/// `Dequeue` implementation based on `MapStorage` and `ValueStorage`s.
///
/// Generic parameters `Key` and `Value` specify data and keys for storing.
/// Generic parameter `Error` requires `DequeueError` implementation.
/// Generic parameter `Callbacks` presents actions for success operations
/// over dequeue.
pub struct DequeueImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    PhantomData<(Error, HVS, TVS, MS, Callbacks)>,
)
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: DequeueCallbacks<Value = Value>;

/// Represents node of the dequeue.
///
/// Contains value and link to the next node.
#[derive(Encode, Decode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct LinkedNode<K, V> {
    /// Key of the next node of dequeue,
    /// if present.
    pub next: Option<K>,
    /// Stored value of current node.
    pub value: V,
}

// Implementation of `Counted` trait for `DequeueImpl` in case,
// when inner `MapStorage` implements `Counted`.
impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Counted
    for DequeueImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>> + Counted,
    Callbacks: DequeueCallbacks<Value = Value>,
{
    type Length = MS::Length;

    fn len() -> Self::Length {
        MS::len()
    }
}

// Implementation of `Dequeue` for `DequeueImpl`.
impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Dequeue
    for DequeueImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: DequeueCallbacks<Value = Value>,
{
    type Key = Key;
    type Value = Value;
    type Error = Error;

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(mut f: F) {
        MS::mutate_values(|n| LinkedNode {
            next: n.next,
            value: f(n.value),
        })
    }

    /// Very expensive operation!
    /// Use dequeue based on double linked list instead!
    fn pop_back() -> Result<Option<Self::Value>, Self::Error> {
        if let Some(head_key) = HVS::get() {
            let tail_key = TVS::take().ok_or_else(Self::Error::tail_should_be_set)?;
            let tail = MS::take(tail_key.clone()).ok_or_else(Self::Error::element_not_found)?;

            let mut next_key = head_key;

            loop {
                let node = MS::get(&next_key).ok_or_else(Self::Error::element_not_found)?;

                if let Some(nodes_next) = node.next {
                    if nodes_next == tail_key {
                        break;
                    }

                    next_key = nodes_next;
                } else {
                    return Err(Self::Error::tail_parent_not_found());
                }
            }

            let mut node = MS::take(next_key.clone()).ok_or_else(Self::Error::element_not_found)?;

            TVS::put(next_key.clone());

            node.next = None;
            MS::insert(next_key, node);

            Callbacks::OnPopBack::call(&tail.value);
            Ok(Some(tail.value))
        } else if TVS::exists() {
            Err(Self::Error::tail_should_not_be_set())
        } else {
            Ok(None)
        }
    }

    fn pop_front() -> Result<Option<Self::Value>, Self::Error> {
        if let Some(head_key) = HVS::take() {
            let LinkedNode { next, value } =
                MS::take(head_key).ok_or_else(Self::Error::element_not_found)?;

            if let Some(next) = next {
                HVS::put(next)
            } else if TVS::take().is_none() {
                return Err(Self::Error::tail_should_be_set());
            }

            Callbacks::OnPopFront::call(&value);
            Ok(Some(value))
        } else if TVS::exists() {
            Err(Self::Error::tail_should_not_be_set())
        } else {
            Ok(None)
        }
    }

    fn push_back(key: Self::Key, value: Self::Value) -> Result<(), Self::Error> {
        if MS::contains_key(&key) {
            Err(Self::Error::duplicate_key())
        } else if let Some(tail_key) = TVS::take() {
            if let Some(mut tail) = MS::take(tail_key.clone()) {
                if tail.next.is_some() {
                    Err(Self::Error::tail_has_next_key())
                } else {
                    TVS::put(key.clone());

                    tail.next = Some(key.clone());
                    MS::insert(tail_key, tail);

                    Callbacks::OnPushBack::call(&value);
                    MS::insert(key, LinkedNode { next: None, value });

                    Ok(())
                }
            } else {
                Err(Self::Error::element_not_found())
            }
        } else if HVS::exists() {
            Err(Self::Error::head_should_not_be_set())
        } else {
            HVS::put(key.clone());
            TVS::put(key.clone());

            Callbacks::OnPushBack::call(&value);
            MS::insert(key, LinkedNode { next: None, value });

            Ok(())
        }
    }

    fn push_front(key: Self::Key, value: Self::Value) -> Result<(), Self::Error> {
        if MS::contains_key(&key) {
            Err(Self::Error::duplicate_key())
        } else if let Some(head_key) = HVS::take() {
            HVS::put(key.clone());

            Callbacks::OnPushFront::call(&value);
            MS::insert(
                key,
                LinkedNode {
                    next: Some(head_key),
                    value,
                },
            );

            Ok(())
        } else if TVS::exists() {
            Err(Self::Error::tail_should_not_be_set())
        } else {
            HVS::put(key.clone());
            TVS::put(key.clone());

            Callbacks::OnPushFront::call(&value);
            MS::insert(key, LinkedNode { next: None, value });

            Ok(())
        }
    }

    fn clear() {
        HVS::kill();
        TVS::kill();
        MS::clear();
        Callbacks::OnClear::call();
    }
}

/// Drain iterator over dequeue's values.
///
/// Removes element on each iteration.
pub struct DequeueDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    Option<Key>,
    PhantomData<(Error, HVS, TVS, MS, Callbacks)>,
)
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: DequeueCallbacks<Value = Value>;

// `Iterator` implementation for `DequeueDrainIter`.
impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Iterator
    for DequeueDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: DequeueCallbacks<Value = Value>,
{
    type Item = Result<Value, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.0.take()?;

        if let Some(node) = MS::take(current) {
            if let Some(k) = node.next.as_ref() {
                HVS::put(k.clone())
            }

            self.0 = node.next;

            Callbacks::OnPopFront::call(&node.value);
            Some(Ok(node.value))
        } else {
            HVS::kill();
            TVS::kill();
            self.0 = None;

            Some(Err(Error::element_not_found()))
        }
    }
}

/// Common iterator over dequeue's values.
pub struct DequeueIter<Key, Value, Error, HVS, TVS, MS>(
    Option<Key>,
    PhantomData<(Error, HVS, TVS, MS)>,
)
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>;

// `Iterator` implementation for `DequeueIter`.
impl<Key, Value, Error, HVS, TVS, MS> Iterator for DequeueIter<Key, Value, Error, HVS, TVS, MS>
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
{
    type Item = Result<Value, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.0.take()?;

        if let Some(node) = MS::get(&current) {
            self.0 = node.next;

            Some(Ok(node.value))
        } else {
            self.0 = None;

            Some(Err(Error::element_not_found()))
        }
    }
}

// `IterableMap` implementation for `DequeueImpl`, returning iterators,
// presented with `DequeueIter` and `DequeueDrainIter`.
impl<Key, Value, Error, HVS, TVS, MS, Callbacks> IterableMap<Result<Value, Error>>
    for DequeueImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: DequeueError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: DequeueCallbacks<Value = Value>,
{
    type DrainIter = DequeueDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>;
    type Iter = DequeueIter<Key, Value, Error, HVS, TVS, MS>;

    fn drain() -> Self::DrainIter {
        DequeueDrainIter(HVS::get(), PhantomData::<(Error, HVS, TVS, MS, Callbacks)>)
    }

    fn iter() -> Self::Iter {
        DequeueIter(HVS::get(), PhantomData::<(Error, HVS, TVS, MS)>)
    }
}
