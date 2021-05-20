use codec::{Decode, Encode};
use sp_core::H256;
use sp_std::prelude::*;

#[derive(Debug, Clone, Encode, Decode)]
struct Node<T: Encode + Decode> {
    value: T,
    next: Option<H256>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct StorageQueue {
    prefix: Vec<u8>,
    head: Option<H256>,
    tail: Option<H256>,
    head_key: Vec<u8>,
    tail_key: Vec<u8>,
}

impl StorageQueue {
    pub fn get(prefix: Vec<u8>) -> StorageQueue {
        let mut head_key = prefix.clone();
        head_key.extend_from_slice(b"head");

        let mut tail_key = prefix.clone();
        tail_key.extend_from_slice(b"tail");

        if let Some(head) = sp_io::storage::get(&head_key) {
            let head = H256::from_slice(&head);
            if let Some(tail) = sp_io::storage::get(&tail_key) {
                let tail = H256::from_slice(&tail);
                StorageQueue {
                    prefix,
                    head: Some(head),
                    tail: Some(tail),
                    head_key,
                    tail_key,
                }
            } else {
                StorageQueue {
                    prefix,
                    head: Some(head),
                    tail: Some(head),
                    head_key,
                    tail_key,
                }
            }
        } else {
            StorageQueue {
                prefix,
                head: None,
                tail: None,
                head_key,
                tail_key,
            }
        }
    }

    pub fn queue<T: Encode + Decode>(&mut self, value: T, id: H256) {
        // let value_key = self.value_key(value, nonce);

        let prefix_value_key = self.key_with_prefix(&id);

        // store value
        sp_io::storage::set(&prefix_value_key, &Node { value, next: None }.encode());

        // update prev value
        if let Some(prev_node_key) = &self.tail {
            if let Some(prev_node) = sp_io::storage::get(&self.key_with_prefix(prev_node_key)) {
                let mut prev_node: Node<T> =
                    Node::<T>::decode(&mut &prev_node[..]).expect("Node<T> decode fail");
                prev_node.next = Some(id);
                sp_io::storage::set(&self.key_with_prefix(prev_node_key), &prev_node.encode());
            }
        }

        // update tail
        self.tail = Some(id);
        sp_io::storage::set(&self.tail_key, &id.to_fixed_bytes());
    }

    pub fn dequeue<T: Encode + Decode>(&mut self) -> Option<T> {
        if self.head == self.tail {
            None
        } else if let Some(value_key) = self.head {
            if let Some(val) = sp_io::storage::get(&self.key_with_prefix(&value_key)) {
                let node: Node<T> = Node::<T>::decode(&mut &val[..]).expect("Node<T> decode fail");
                sp_io::storage::clear(&self.key_with_prefix(&value_key));
                if let Some(next) = node.next {
                    sp_io::storage::set(&self.head_key, &next.to_fixed_bytes());
                    self.head = Some(next);
                } else {
                    sp_io::storage::clear(&self.head_key);
                    self.head = None;
                }
                Some(node.value)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn key_with_prefix(&self, key: &H256) -> Vec<u8> {
        let mut prefix_key = self.prefix.clone();
        prefix_key.extend(key.to_fixed_bytes());
        prefix_key
    }
}
