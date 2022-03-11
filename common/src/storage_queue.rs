// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use codec::{Codec, Decode, Encode};
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_std::borrow::Cow;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

fn key_with_prefix(prefix: &[u8], key: &[u8]) -> Vec<u8> {
    [prefix, key].concat()
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
struct Node<T: Codec> {
    value: T,
    next: Option<H256>,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct StorageQueue<T: Codec> {
    prefix: Cow<'static, [u8]>,
    head: Option<H256>,
    tail: Option<H256>,
    _marker: PhantomData<T>,
}

impl<T: Codec> StorageQueue<T> {
    pub fn get(prefix: impl Into<Cow<'static, [u8]>>) -> Self {
        let prefix: Cow<'static, [u8]> = prefix.into();

        let head_key = [prefix.as_ref(), b"head"].concat();
        let tail_key = [prefix.as_ref(), b"tail"].concat();

        if let Some(head) = sp_io::storage::get(&head_key) {
            let head = H256::from_slice(&head);
            if let Some(tail) = sp_io::storage::get(&tail_key) {
                let tail = H256::from_slice(&tail);
                Self {
                    prefix,
                    head: Some(head),
                    tail: Some(tail),
                    _marker: Default::default(),
                }
            } else {
                Self {
                    prefix,
                    head: Some(head),
                    tail: Some(head),
                    _marker: Default::default(),
                }
            }
        } else {
            Self {
                prefix,
                head: None,
                tail: None,
                _marker: Default::default(),
            }
        }
    }

    pub fn queue(&mut self, value: T, id: H256) {
        // store value
        sp_io::storage::set(
            &self.key_with_prefix(id.as_bytes()),
            &Node { value, next: None }.encode(),
        );

        // update prev value
        if let Some(prev_node_key) = &self.tail {
            if let Some(prev_node) =
                sp_io::storage::get(&self.key_with_prefix(prev_node_key.as_bytes()))
            {
                let mut prev_node: Node<T> =
                    Node::<T>::decode(&mut &prev_node[..]).expect("Node<T> decode fail");
                prev_node.next = Some(id);
                sp_io::storage::set(
                    &self.key_with_prefix(prev_node_key.as_bytes()),
                    &prev_node.encode(),
                );
            }
        }

        // set head if queue was empty
        if self.is_empty() {
            self.set_head(id);
        }

        // update tail
        self.set_tail(id);
    }

    pub fn dequeue(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else if let Some(value_key) = self.head {
            if let Some(val) = sp_io::storage::get(&self.key_with_prefix(value_key.as_bytes())) {
                let node: Node<T> = Node::<T>::decode(&mut &val[..]).expect("Node<T> decode fail");
                sp_io::storage::clear(&self.key_with_prefix(value_key.as_bytes()));
                if let Some(next) = node.next {
                    self.set_head(next);
                } else {
                    sp_io::storage::clear(&self.key_with_prefix(b"head"));
                    sp_io::storage::clear(&self.key_with_prefix(b"tail"));
                    self.head = None;
                    self.tail = None;
                }
                Some(node.value)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none() && self.tail.is_none()
    }

    fn set_head(&mut self, id: H256) {
        self.head = Some(id);
        sp_io::storage::set(&self.key_with_prefix(b"head"), &id.to_fixed_bytes());
    }

    fn set_tail(&mut self, id: H256) {
        self.tail = Some(id);
        sp_io::storage::set(&self.key_with_prefix(b"tail"), &id.to_fixed_bytes());
    }

    fn key_with_prefix(&self, key: &[u8]) -> Vec<u8> {
        key_with_prefix(self.prefix.as_ref(), key)
    }
}

#[derive(Debug, Clone)]
pub struct Iterator<T: Codec>(Option<H256>, Cow<'static, [u8]>, PhantomData<T>);

impl<T: Codec> sp_std::iter::Iterator for Iterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let Iterator(head, prefix, ..) = self;

        let (result, next_head) = head
            .and_then(|value_key| {
                sp_io::storage::get(&key_with_prefix(prefix.as_ref(), value_key.as_bytes())).map(
                    |value| {
                        let node = Node::<T>::decode(&mut &value[..]).expect("Node<T> decode fail");

                        (Some(node.value), node.next)
                    },
                )
            })
            .unwrap_or_default();

        self.0 = next_head;

        result
    }
}

impl<T: Codec> sp_std::iter::IntoIterator for StorageQueue<T> {
    type Item = T;
    type IntoIter = Iterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        Iterator(self.head, self.prefix, Default::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::get(b"test::queue::".as_ref());

            assert!(queue.is_empty());

            let value: Option<u8> = queue.dequeue();
            assert!(value.is_none());
        });
    }

    #[test]
    fn last_element() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::get(b"test::queue::".as_ref());

            queue.queue(0u32, H256::random());
            let value: Option<u32> = queue.dequeue();

            assert_eq!(value, Some(0u32));
        });
    }

    #[test]
    fn fifo() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::get(b"test::queue::".as_ref());

            (0..10u32).for_each(|x| queue.queue(x, H256::random()));

            (0..10u32).for_each(|x| {
                let value: Option<u32> = queue.dequeue();
                assert_eq!(Some(x), value);
            });

            let value: Option<u32> = queue.dequeue();
            assert!(value.is_none());
        });
    }
}
