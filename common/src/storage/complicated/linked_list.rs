use crate::storage::primitives::{Callback, Counted, EmptyCallback, MapStorage, ValueStorage};
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

pub struct LinkedListDrain<Key, Value, Error, HVS, TVS, MS, Callbacks>(
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
    for LinkedListDrain<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
{
    type Item = Result<(Key, Value), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.0.take()?;

        if let Some(node) = MS::take(current.clone()) {
            self.0 = node.next;

            Some(Ok((current, node.value)))
        } else {
            self.0 = None;

            Some(Err(Error::element_not_found()))
        }
    }
}

pub struct LinkedListIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>(
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
    for LinkedListIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
{
    type Item = Result<(Key, Value), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.0.take()?;

        if let Some(node) = MS::get(&current) {
            self.0 = node.next;

            Some(Ok((current, node.value)))
        } else {
            self.0 = None;

            Some(Err(Error::element_not_found()))
        }
    }
}

pub struct LinkedListKeyIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    LinkedListIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>,
)
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>;

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Iterator
    for LinkedListKeyIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>,
{
    type Item = Result<Key, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|r| r.map(|v| v.0))
    }
}

pub struct LinkedListValueIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>(
    LinkedListIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>,
)
where
    Key: Clone + PartialEq,
    Error: LinkedListError,
    HVS: ValueStorage<Value = Key>,
    TVS: ValueStorage<Value = Key>,
    MS: MapStorage<Key = Key, Value = LinkedNode<Key, Value>>,
    Callbacks: LinkedListCallbacks<Value = Value>;

impl<Key, Value, Error, HVS, TVS, MS, Callbacks> Iterator
    for LinkedListValueIterator<Key, Value, Error, HVS, TVS, MS, Callbacks>
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
        self.0.next().map(|r| r.map(|v| v.1))
    }
}
