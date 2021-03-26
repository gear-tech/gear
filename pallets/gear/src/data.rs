use codec::{Encode, Decode};
use sp_core::H256;
use sp_std::prelude::*;

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Message {
    pub source: H256,
    pub dest: H256,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Program {
    pub static_pages: Vec<u8>,
    pub code: Vec<u8>,
}
