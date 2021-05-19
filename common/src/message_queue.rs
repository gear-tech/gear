use crate::Message;
use codec::{Decode, Encode};
use sp_std::convert::TryInto;
use sp_std::prelude::*;

fn read_le_u32(input: &mut &[u8]) -> u32 {
    let (int_bytes, rest) = input.split_at(sp_std::mem::size_of::<u32>());
    *input = rest;
    u32::from_le_bytes(int_bytes.try_into().unwrap())
}

pub struct MessageQueue {
    prefix: Vec<u8>,
    head: u32,
    tail: u32,
    head_id: Vec<u8>,
    tail_id: Vec<u8>,
}

impl MessageQueue {
    pub fn get(prefix: Vec<u8>) -> MessageQueue {
        let mut head_id = prefix.clone();
        head_id.extend_from_slice(b"head");

        let mut tail_id = prefix.clone();
        tail_id.extend_from_slice(b"tail");

        if let Some(head) = sp_io::storage::get(&head_id) {
            let head: u32 = read_le_u32(&mut head.as_slice());
            if let Some(tail) = sp_io::storage::get(&tail_id) {
                let tail: u32 = read_le_u32(&mut tail.as_slice());
                MessageQueue {
                    prefix,
                    head,
                    tail,
                    head_id,
                    tail_id,
                }
            } else {
                MessageQueue {
                    prefix,
                    head,
                    tail: head,
                    head_id,
                    tail_id,
                }
            }
        } else {
            let head: u32 = 0;
            let tail: u32 = 0;

            sp_io::storage::set(&head_id, &head.to_le_bytes());
            sp_io::storage::set(&tail_id, &tail.to_le_bytes());
            MessageQueue {
                prefix,
                head,
                tail,
                head_id,
                tail_id,
            }
        }
    }

    pub fn queue(&mut self, value: Message) {
        // gen message key
        let mut message_key = self.prefix.clone();
        message_key.extend_from_slice(&self.tail.to_le_bytes());

        // store message
        sp_io::storage::set(&self.tail_id, &value.encode());

        // update tail
        self.tail = self.tail.wrapping_add(1);
        sp_io::storage::set(&message_key, &self.tail.to_le_bytes());
    }

    pub fn dequeue(&mut self) -> Option<Message> {
        if self.head == self.tail {
            None
        } else {

            let mut message_key = self.prefix.clone();
            message_key.extend_from_slice(&self.head.to_le_bytes());

            if let Some(msg) = sp_io::storage::get(&message_key) {
                sp_io::storage::clear(&self.head_id);
                self.head = self.head.wrapping_add(1);
                sp_io::storage::set(&self.head_id, &self.head.to_le_bytes());
                Some(Message::decode(&mut &msg[..]).expect("Message::decode fail"))
            } else {
                None
            }
        }
    }
}
