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
use scale_info::TypeInfo;
use sp_std::borrow::Cow;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

fn key_with_prefix(prefix: &[u8], key: &[u8]) -> Vec<u8> {
    [prefix, key].concat()
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
struct Node<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec> {
    value: V,
    next: Option<K>,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct StorageQueue<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec> {
    prefix: Cow<'static, [u8]>,
    head: Option<K>,
    tail: Option<K>,
    _phantom: PhantomData<V>,
}

impl<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec> StorageQueue<K, V> {
    pub fn get(prefix: impl Into<Cow<'static, [u8]>>) -> Self {
        let prefix: Cow<'static, [u8]> = prefix.into();

        let head_key = [prefix.as_ref(), b"head"].concat();
        let tail_key = [prefix.as_ref(), b"tail"].concat();

        if let Some(head) = sp_io::storage::get(&head_key) {
            let mut arr = [0; 32];
            arr.copy_from_slice(&head);
            let head = arr.into();
            if let Some(tail) = sp_io::storage::get(&tail_key) {
                let mut arr = [0; 32];
                arr.copy_from_slice(&tail);
                let tail = arr.into();
                Self {
                    prefix,
                    head: Some(head),
                    tail: Some(tail),
                    _phantom: Default::default(),
                }
            } else {
                Self {
                    prefix,
                    head: Some(head),
                    tail: Some(head),
                    _phantom: Default::default(),
                }
            }
        } else {
            Self {
                prefix,
                head: None,
                tail: None,
                _phantom: Default::default(),
            }
        }
    }

    pub fn queue(&mut self, id: K, value: V) {
        // store value
        sp_io::storage::set(
            &self.key_with_prefix(id.as_ref()),
            &Node {
                value,
                next: Option::<K>::None,
            }
            .encode(),
        );

        // update prev value
        if let Some(prev_node_key) = &self.tail {
            if let Some(prev_node) =
                sp_io::storage::get(&self.key_with_prefix(prev_node_key.as_ref()))
            {
                let mut prev_node: Node<K, V> =
                    Node::<K, V>::decode(&mut &prev_node[..]).expect("Node<K, V> decode fail");
                prev_node.next = Some(id);
                sp_io::storage::set(
                    &self.key_with_prefix(prev_node_key.as_ref()),
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

    pub fn dequeue(&mut self) -> Option<V> {
        if self.is_empty() {
            None
        } else if let Some(value_key) = self.head {
            if let Some(val) = sp_io::storage::get(&self.key_with_prefix(value_key.as_ref())) {
                let node: Node<K, V> =
                    Node::<K, V>::decode(&mut &val[..]).expect("Node<T> decode fail");
                sp_io::storage::clear(&self.key_with_prefix(value_key.as_ref()));
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

    fn set_head(&mut self, id: K) {
        self.head = Some(id);
        sp_io::storage::set(&self.key_with_prefix(b"head"), &id.encode());
    }

    fn set_tail(&mut self, id: K) {
        self.tail = Some(id);
        sp_io::storage::set(&self.key_with_prefix(b"tail"), &id.encode());
    }

    fn key_with_prefix(&self, key: &[u8]) -> Vec<u8> {
        key_with_prefix(self.prefix.as_ref(), key)
    }
}

#[derive(Debug, Clone)]
pub struct Iterator<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec>(
    Option<K>,
    Cow<'static, [u8]>,
    PhantomData<V>,
);

impl<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec> sp_std::iter::Iterator
    for Iterator<K, V>
{
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        let Iterator(head, prefix, ..) = self;

        let (result, next_head) = head
            .and_then(|value_key| {
                sp_io::storage::get(&key_with_prefix(prefix.as_ref(), value_key.as_ref())).map(
                    |value| {
                        let node =
                            Node::<K, V>::decode(&mut &value[..]).expect("Node<K, V> decode fail");

                        (Some(node.value), node.next)
                    },
                )
            })
            .unwrap_or_default();

        self.0 = next_head;

        result
    }
}

impl<K: Copy + From<[u8; 32]> + AsRef<[u8]> + Codec, V: Codec> sp_std::iter::IntoIterator
    for StorageQueue<K, V>
{
    type Item = V;
    type IntoIter = Iterator<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iterator(self.head, self.prefix, Default::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitive_types::H256;

    #[test]
    fn empty_queue() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::<H256, u8>::get(b"test::queue::".as_ref());

            assert!(queue.is_empty());

            let value: Option<u8> = queue.dequeue();
            assert!(value.is_none());
        });
    }

    #[test]
    fn last_element() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::<H256, u32>::get(b"test::queue::".as_ref());

            queue.queue(H256::random(), 0u32);
            let value: Option<u32> = queue.dequeue();

            assert_eq!(value, Some(0u32));
        });
    }

    #[test]
    fn fifo() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let mut queue = StorageQueue::<H256, u32>::get(b"test::queue::".as_ref());

            (0..10u32).for_each(|x| queue.queue(H256::random(), x));

            (0..10u32).for_each(|x| {
                let value: Option<u32> = queue.dequeue();
                assert_eq!(Some(x), value);
            });

            let value: Option<u32> = queue.dequeue();
            assert!(value.is_none());
        });
    }
}
