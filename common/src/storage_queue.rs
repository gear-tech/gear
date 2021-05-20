use codec::{Decode, Encode};
use sp_core::{blake2_256, H256};
use sp_std::convert::TryInto;
use sp_std::prelude::*;

fn read_le_u32(input: &mut &[u8]) -> u32 {
    let (int_bytes, rest) = input.split_at(4);
    *input = rest;
    u32::from_le_bytes(int_bytes.try_into().unwrap())
}

#[derive(Debug, Clone, Encode, Decode)]
struct Link<T: Encode + Decode> {
    value: T,
    next: Option<Vec<u8>>,
}

pub struct StorageQueue {
    prefix: Vec<u8>,
    head: Option<Vec<u8>>,
    tail: Option<Vec<u8>>,
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
            let head = H256::from_slice(&head).to_fixed_bytes().to_vec();
            if let Some(tail) = sp_io::storage::get(&tail_key) {
                let tail = H256::from_slice(&tail).to_fixed_bytes().to_vec();
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
            // sp_io::storage::set(&head_key, &head.to_le_bytes());
            // sp_io::storage::set(&tail_key, &tail.to_le_bytes());
            StorageQueue {
                prefix,
                head: None,
                tail: None,
                head_key,
                tail_key,
            }
        }
    }

    pub fn queue<T: Encode + Decode>(&mut self, value: T, nonce: u128) {
        let value_key = self.value_key(value, nonce);

        // store value
        sp_io::storage::set(&value_key, &Link { value, next: None }.encode());

        // update prev value
        if let Some(prev_link_key) = &self.tail {
            if let Some(prev_link) = sp_io::storage::get(prev_link_key) {
                let mut prev_link: Link<T> =
                    Link::<T>::decode(&mut &prev_link[..]).expect("Link<T> decode fail");
                prev_link.next = Some(value_key);
                sp_io::storage::set(prev_link_key, &prev_link.encode());
            }
        }

        // update tail
        self.tail = Some(value_key);
        sp_io::storage::set(&self.tail_key, &value_key);
    }

    pub fn dequeue<T: Encode + Decode>(&mut self) -> Option<T> {
        if self.head == self.tail {
            None
        } else if let Some(value_key) = self.head {
            if let Some(val) = sp_io::storage::get(&value_key) {
                let link: Link<T> = Link::<T>::decode(&mut &val[..]).expect("Link<T> decode fail");
                let value = link.value;
                sp_io::storage::clear(&value_key);
                if let Some(next) = link.next {
                    sp_io::storage::set(&self.head_key, &next);
                    self.head = Some(next);
                } else {
                    sp_io::storage::clear(&self.head_key);
                    self.head = None;
                }
                Some(value)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn value_key<T: Encode>(&self, value: T, nonce: u128) -> Vec<u8> {
        let mut prefix = self.prefix.clone();
        let mut data = value.encode();
        data.extend_from_slice(&nonce.to_le_bytes());
        let hash = blake2_256(&data);
        let hash = H256::from_slice(&hash);
        prefix.extend_from_slice(&hash.to_fixed_bytes());
        prefix
    }
}
