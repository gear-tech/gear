extern crate alloc;

use alloc::vec::Vec;

#[derive(Clone, Debug, codec::Encode, codec::Decode)]
pub enum RoomMessage {
    Join { under_name: Vec<u8> },
    Yell { text: Vec<u8> },
}

#[derive(Clone, Debug, codec::Encode, codec::Decode)]
pub enum MemberMessage {
    Private(Vec<u8>),
    Room(Vec<u8>),
}
