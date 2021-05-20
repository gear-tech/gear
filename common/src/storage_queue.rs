use codec::{Decode, Encode};
use sp_std::convert::TryInto;
use sp_std::prelude::*;

fn read_le_u32(input: &mut &[u8]) -> u32 {
    let (int_bytes, rest) = input.split_at(4);
    *input = rest;
    u32::from_le_bytes(int_bytes.try_into().unwrap())
}

pub struct StorageQueue {
    prefix: Vec<u8>,
    head: u32,
    tail: u32,
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
            let head: u32 = read_le_u32(&mut head.as_slice());
            if let Some(tail) = sp_io::storage::get(&tail_key) {
                let tail: u32 = read_le_u32(&mut tail.as_slice());
                StorageQueue {
                    prefix,
                    head,
                    tail,
                    head_key,
                    tail_key,
                }
            } else {
                StorageQueue {
                    prefix,
                    head,
                    tail: head,
                    head_key,
                    tail_key,
                }
            }
        } else {
            let head: u32 = 0;
            let tail: u32 = 0;

            sp_io::storage::set(&head_key, &head.to_le_bytes());
            sp_io::storage::set(&tail_key, &tail.to_le_bytes());
            StorageQueue {
                prefix,
                head,
                tail,
                head_key,
                tail_key,
            }
        }
    }

    pub fn queue<T: Encode>(&mut self, value: T) {
        let value_key = self.value_key(self.tail);

        // store message
        sp_io::storage::set(&value_key, &value.encode());

        // update tail
        self.tail = self.tail.wrapping_add(1);
        sp_io::storage::set(&self.tail_key, &self.tail.to_le_bytes());
    }

    pub fn dequeue<T: Decode>(&mut self) -> Option<T> {
        if self.head == self.tail {
            None
        } else {
            let value_key = self.value_key(self.head);

            if let Some(val) = sp_io::storage::get(&value_key) {
                sp_io::storage::clear(&self.head_key);
                self.head = self.head.wrapping_add(1);
                sp_io::storage::set(&self.head_key, &self.head.to_le_bytes());
                Some(T::decode(&mut &val[..]).expect("decode fail"))
            } else {
                None
            }
        }
    }

    fn value_key(&self, id: u32) -> Vec<u8> {
        let mut value_key = self.prefix.clone();
        value_key.extend_from_slice(&id.to_le_bytes());
        value_key
    }
}
