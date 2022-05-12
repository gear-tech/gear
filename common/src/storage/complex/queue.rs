use crate::storage::complicated::LinkedList;
use crate::storage::primitives::{Counted, KeyFor};
use core::marker::PhantomData;

pub trait Queue {
    type Value;
    type Error;

    fn dequeue() -> Result<Option<Self::Value>, Self::Error>;

    fn queue(value: Self::Value) -> Result<(), Self::Error>;

    fn requeue(value: Self::Value) -> Result<(), Self::Error>;

    fn remove_all();
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

    fn queue(value: Self::Value) -> Result<(), Self::Error> {
        let key = KeyGen::key_for(&value);
        T::push_back(key, value)
    }

    fn requeue(value: Self::Value) -> Result<(), Self::Error> {
        let key = KeyGen::key_for(&value);
        T::push_front(key, value)
    }

    fn remove_all() {
        T::remove_all()
    }
}

impl<T: LinkedList + Counted, KeyGen: KeyFor<Key = T::Key, Value = T::Value>> Counted
    for QueueImpl<T, KeyGen>
{
    type Length = T::Length;

    fn len() -> Self::Length {
        T::len()
    }
}
