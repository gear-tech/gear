use super::*;
use codec::{Decode, Encode};
use core::{iter::Iterator, marker::PhantomData};
use scale_info::TypeInfo;

pub enum DequeError {
    HeadNotFoundInElements,
    HeadWasNotRemoved,
    TailWasEmptyWhileHeadNot,
    HeadWasEmptyWhileTailNot,
    DuplicateElementKey,
    ElementNotFound,
    TailHadNextPointer,
    TailWasNotRemoved,
}

pub trait NextKey<V> {
    fn first(target: &V) -> Self;
    fn next(&self, target: &V) -> Self;
}

#[derive(Decode, Encode, TypeInfo)]
pub struct Node<K, V> {
    pub next: Option<K>,
    pub value: V,
}

/// Stripped double-ended queue implementation based on map-storage.
pub trait StorageDeque: Sized {
    type Key: Clone + NextKey<Self::Value>;
    type Value;

    type Error: From<DequeError>;

    type HeadKey: StorageValue<Value = Self::Key>;
    type TailKey: StorageValue<Value = Self::Key>;
    type Elements: StorageMap<Key = Self::Key, Value = Node<Self::Key, Self::Value>>;
    type Length: StorageCounter;

    type OnPopFront: Callback<Self::Value>;
    type OnPushFront: Callback<Self::Value>;
    type OnPushBack: Callback<Self::Value>;

    fn pop_front() -> Result<Option<Self::Value>, Self::Error> {
        if let Some(head_key) = Self::HeadKey::remove() {
            if let Some(Node {
                next: next_opt,
                value: head,
            }) = Self::Elements::remove(head_key)
            {
                Self::OnPopFront::call(&head);

                if let Some(next) = next_opt {
                    if Self::HeadKey::set(next).is_some() {
                        return Err(DequeError::HeadWasNotRemoved.into());
                    }
                } else if Self::TailKey::remove().is_none() {
                    return Err(DequeError::TailWasEmptyWhileHeadNot.into());
                }

                Self::Length::decrease();
                Ok(Some(head))
            } else {
                Err(DequeError::HeadNotFoundInElements.into())
            }
        } else {
            Ok(None)
        }
    }

    fn push_back(value: Self::Value) -> Result<(), Self::Error> {
        Self::OnPushBack::call(&value);

        if let Some(tail_key) = Self::TailKey::remove() {
            let mut new_tail_key = tail_key.next(&value);

            while Self::Elements::contains(&new_tail_key) {
                new_tail_key = new_tail_key.next(&value);
            }

            if Self::TailKey::set(new_tail_key.clone()).is_some() {
                return Err(DequeError::TailWasNotRemoved.into());
            }

            Self::Elements::mutate(tail_key, |n| {
                if let Some(n) = n {
                    if n.next.is_some() {
                        Err(DequeError::TailHadNextPointer)
                    } else {
                        n.next = Some(new_tail_key.clone());
                        Ok(())
                    }
                } else {
                    Err(DequeError::ElementNotFound)
                }
            })
            .map_err(Into::into)?;

            if Self::Elements::set(new_tail_key, Node { next: None, value }).is_some() {
                return Err(DequeError::DuplicateElementKey.into());
            }
        } else {
            let first_key = Self::Key::first(&value);

            let _ = Self::TailKey::set(first_key.clone());

            if Self::HeadKey::set(first_key.clone()).is_some() {
                return Err(DequeError::TailWasEmptyWhileHeadNot.into());
            }

            if Self::Elements::set(first_key, Node { next: None, value }).is_some() {
                return Err(DequeError::DuplicateElementKey.into());
            }
        }

        Self::Length::increase();
        Ok(())
    }

    fn push_front(value: Self::Value) -> Result<(), Self::Error> {
        Self::OnPushFront::call(&value);

        if let Some(head_key) = Self::HeadKey::remove() {
            let mut new_head_key = head_key.next(&value);

            while Self::Elements::contains(&new_head_key) {
                new_head_key = new_head_key.next(&value);
            }

            if Self::HeadKey::set(new_head_key.clone()).is_some() {
                return Err(DequeError::HeadWasNotRemoved.into());
            }

            if Self::Elements::set(
                new_head_key,
                Node {
                    next: Some(head_key),
                    value,
                },
            )
            .is_some()
            {
                return Err(DequeError::DuplicateElementKey.into());
            }
        } else if Self::TailKey::remove().is_some() {
            // This branch occurs only in case of algorithmic error
            // or storage corruption, but should be also handled.
            return Err(DequeError::HeadWasEmptyWhileTailNot.into());
        } else {
            let first_key = Self::Key::first(&value);

            let _ = Self::TailKey::set(first_key.clone());

            if Self::HeadKey::set(first_key.clone()).is_some() {
                return Err(DequeError::HeadWasEmptyWhileTailNot.into());
            }

            if Self::Elements::set(first_key, Node { next: None, value }).is_some() {
                return Err(DequeError::DuplicateElementKey.into());
            }
        }

        Self::Length::increase();
        Ok(())
    }

    fn iter() -> StorageDequeIterator<Self, Self::Error> {
        StorageDequeIterator(Self::HeadKey::get(), Default::default())
    }

    fn clear() -> Result<(), Self::Error> {
        let _ = Self::TailKey::remove();
        let mut next_opt = Self::HeadKey::remove();

        while let Some(next) = next_opt {
            if let Some(node) = Self::Elements::remove(next) {
                next_opt = node.next;
            } else {
                return Err(DequeError::ElementNotFound.into());
            }
        }

        Self::Length::clear();

        Ok(())
    }

    fn mutate_all(
        mutator: impl FnOnce(Self::Value) -> Self::Value + Copy,
    ) -> Result<(), Self::Error> {
        let mut next_opt = Self::HeadKey::get();

        while let Some(next) = next_opt {
            if let Some(mut node) = Self::Elements::remove(next.clone()) {
                next_opt = node.next.clone();

                node.value = mutator(node.value);

                Self::Elements::set(next, node);
            } else {
                return Err(DequeError::ElementNotFound.into());
            }
        }

        Ok(())
    }

    fn is_empty() -> Result<bool, Self::Error> {
        let head = Self::HeadKey::get();
        let tail = Self::TailKey::get();

        match (head, tail) {
            (Some(_), Some(_)) => Ok(false),
            (None, None) => Ok(true),
            (Some(_), None) => Err(DequeError::TailWasEmptyWhileHeadNot.into()),
            (None, Some(_)) => Err(DequeError::HeadWasEmptyWhileTailNot.into()),
        }
    }
}

/// Iterator over given StorageDeque implementation.
#[derive(Debug, Clone)]
pub struct StorageDequeIterator<Deque, E>(Option<Deque::Key>, PhantomData<E>)
where
    Deque: StorageDeque,
    E: From<DequeError>;

impl<Deque: StorageDeque, E: From<DequeError>> Iterator for StorageDequeIterator<Deque, E> {
    type Item = Result<Deque::Value, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.0.clone()?;

        if let Some(node) = Deque::Elements::get(&next) {
            self.0 = node.next;

            Some(Ok(node.value))
        } else {
            self.0 = None;

            Some(Err(DequeError::ElementNotFound.into()))
        }
    }
}
