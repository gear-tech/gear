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

use crate::storage::{Callback, Counted, EmptyCallback, IterableMap, MapStorage, ValueStorage};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use scale_info::TypeInfo;

pub trait LinkedListCallbacks {
    type Value;

    type OnPopBack: Callback<Self::Value>;
    type OnPopFront: Callback<Self::Value>;
    type OnPushBack: Callback<Self::Value>;
    type OnPushFront: Callback<Self::Value>;
    type OnRemoveAll: EmptyCallback;
}

pub trait LinkedListError {
    fn duplicate_key() -> Self;

    fn element_not_found() -> Self;

    fn head_should_be() -> Self;

    fn head_should_not_be() -> Self;

    fn tail_has_next_key() -> Self;

    fn tail_parent_not_found() -> Self;

    fn tail_should_be() -> Self;

    fn tail_should_not_be() -> Self;
}

#[derive(Encode, Decode, TypeInfo)]
pub struct LinkedNode<K, V> {
    pub next: Option<K>,
    pub value: V,
}

pub trait LinkedList {
    type Key;
    type Value;
    type Error;

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(f: F);

    // Very expensive operation! Use DoubleLinkedList instead!
    fn pop_back() -> Result<Option<Self::Value>, Self::Error>;

    fn pop_front() -> Result<Option<Self::Value>, Self::Error>;

    fn push_back(key: Self::Key, value: Self::Value) -> Result<(), Self::Error>;

    fn push_front(key: Self::Key, value: Self::Value) -> Result<(), Self::Error>;

    fn remove_all();
}

pub struct LinkedListImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    PhantomData<(Error, HVS, TVS, MS, Callbacks)>,
)
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>;

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Counted
    for LinkedListImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>> + Counted,
    Callbacks: LinkedListCallbacks<Value = Value>,
{
    type Length = MS::Length;

    fn len() -> Self::Length {
        MS::len()
    }
}

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> LinkedList
    for LinkedListImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
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

    // Very expensive operation! Use DoubleLinkedList instead!
    fn pop_back() -> Result<Option<Self::Value>, Self::Error> {
        if let Some(head_key) = HVS::get() {
            let tail_key = TVS::take().ok_or_else(Self::Error::tail_should_be)?;
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
            Err(Self::Error::tail_should_not_be())
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
            } else {
                TVS::kill()
            }

            Callbacks::OnPopFront::call(&value);
            Ok(Some(value))
        } else if TVS::exists() {
            Err(Self::Error::tail_should_not_be())
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
            Err(Self::Error::head_should_not_be())
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
            Err(Self::Error::tail_should_not_be())
        } else {
            HVS::put(key.clone());
            TVS::put(key.clone());

            Callbacks::OnPushFront::call(&value);
            MS::insert(key, LinkedNode { next: None, value });

            Ok(())
        }
    }

    fn remove_all() {
        HVS::kill();
        TVS::kill();
        MS::remove_all();
        Callbacks::OnRemoveAll::call();
    }
}

pub struct LinkedListDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    Option<Key>,
    PhantomData<(Error, HVS, TVS, MS, Callbacks)>,
)
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>;

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Iterator
    for LinkedListDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
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

pub struct LinkedListIter<Key, Value, Error, HVS, TVS, MS>(
    Option<Key>,
    PhantomData<(Error, HVS, TVS, MS)>,
)
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>;

impl<Key, Value, Error, HVS, TVS, MS> Iterator for LinkedListIter<Key, Value, Error, HVS, TVS, MS>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
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

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> IterableMap<Result<Value, Error>>
    for LinkedListImpl<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
{
    type DrainIter = LinkedListDrainIter<Key, Value, Error, HVS, TVS, MS, Callbacks>;
    type Iter = LinkedListIter<Key, Value, Error, HVS, TVS, MS>;

    fn drain() -> Self::DrainIter {
        LinkedListDrainIter(HVS::get(), PhantomData::<(Error, HVS, TVS, MS, Callbacks)>)
    }

    fn iter() -> Self::Iter {
        LinkedListIter(HVS::get(), PhantomData::<(Error, HVS, TVS, MS)>)
    }
}
