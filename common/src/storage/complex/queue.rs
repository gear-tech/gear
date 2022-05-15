use crate::storage::complicated::LinkedList;
use crate::storage::primitives::{Counted, IterableMap, KeyFor};
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
