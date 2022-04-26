use super::*;
use codec::{Decode, Encode};
use core::{iter::Iterator, marker::PhantomData};
use scale_info::TypeInfo;

pub enum DeckError {
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

/// Stripped deck implementation based on map-storage.
pub trait StorageDeck: Sized {
    type Key: Clone + NextKey<Self::Value>;
    type Value;

    type Error: From<DeckError>;

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
                        return Err(DeckError::HeadWasNotRemoved.into());
                    }
                } else if Self::TailKey::remove().is_none() {
                    return Err(DeckError::TailWasEmptyWhileHeadNot.into());
                }

                Self::Length::decrease();
                Ok(Some(head))
            } else {
                Err(DeckError::HeadNotFoundInElements.into())
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
                return Err(DeckError::TailWasNotRemoved.into());
            }

            Self::Elements::mutate(tail_key, |n| {
                if let Some(n) = n {
                    if n.next.is_some() {
                        Err(DeckError::TailHadNextPointer)
                    } else {
                        n.next = Some(new_tail_key.clone());
                        Ok(())
                    }
                } else {
                    Err(DeckError::ElementNotFound)
                }
            })
            .map_err(Into::into)?;

            if Self::Elements::set(new_tail_key, Node { next: None, value }).is_some() {
                return Err(DeckError::DuplicateElementKey.into());
            }
        } else {
            let first_key = Self::Key::first(&value);

            let _ = Self::TailKey::set(first_key.clone());

            if Self::HeadKey::set(first_key.clone()).is_some() {
                return Err(DeckError::HeadWasEmptyWhileTailNot.into());
            }

            if Self::Elements::set(first_key, Node { next: None, value }).is_some() {
                return Err(DeckError::DuplicateElementKey.into());
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
                return Err(DeckError::HeadWasNotRemoved.into());
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
                return Err(DeckError::DuplicateElementKey.into());
            }
        } else if let Some(tail_key) = Self::TailKey::remove() {
            let mut new_tail_key = tail_key.next(&value);

            while Self::Elements::contains(&new_tail_key) {
                new_tail_key = new_tail_key.next(&value);
            }

            if Self::TailKey::set(new_tail_key.clone()).is_some() {
                return Err(DeckError::TailWasNotRemoved.into());
            }

            Self::Elements::mutate(tail_key, |n| {
                if let Some(n) = n {
                    if n.next.is_some() {
                        Err(DeckError::TailHadNextPointer)
                    } else {
                        n.next = Some(new_tail_key.clone());
                        Ok(())
                    }
                } else {
                    Err(DeckError::ElementNotFound)
                }
            })
            .map_err(Into::into)?;

            if Self::Elements::set(new_tail_key, Node { next: None, value }).is_some() {
                return Err(DeckError::DuplicateElementKey.into());
            }
        } else {
            let first_key = Self::Key::first(&value);

            let _ = Self::TailKey::set(first_key.clone());

            if Self::HeadKey::set(first_key.clone()).is_some() {
                return Err(DeckError::HeadWasEmptyWhileTailNot.into());
            }

            if Self::Elements::set(first_key, Node { next: None, value }).is_some() {
                return Err(DeckError::DuplicateElementKey.into());
            }
        }

        Self::Length::increase();
        Ok(())
    }

    fn iter() -> StorageDeckIterator<Self, Self::Error> {
        StorageDeckIterator(Self::HeadKey::get(), Default::default())
    }
}

/// Iterator over given StorageDeck implementation.
#[derive(Debug, Clone)]
pub struct StorageDeckIterator<Deck, E>(Option<Deck::Key>, PhantomData<E>)
where
    Deck: StorageDeck,
    E: From<DeckError>;

impl<Deck: StorageDeck, E: From<DeckError>> Iterator for StorageDeckIterator<Deck, E> {
    type Item = Result<Deck::Value, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.0.clone()?;

        if let Some(node) = Deck::Elements::get(&next) {
            self.0 = node.next;

            Some(Ok(node.value))
        } else {
            self.0 = None;

            Some(Err(DeckError::ElementNotFound.into()))
        }
    }
}
