#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature="std")]
pub mod native;

use codec::{Encode, Decode};
use sp_core::H256;
use sp_std::prelude::*;

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Message {
    pub source: H256,
    pub dest: H256,
    pub payload: Vec<u8>,
    pub gas_limit: u64,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Program {
    pub static_pages: Vec<u8>,
    pub code: Vec<u8>,
}

pub trait IntoOrigin {
    fn into_origin(self) -> H256;
}

impl IntoOrigin for u64 {
    fn into_origin(self) -> H256 {
        let mut result = H256::zero();
        result[0..8].copy_from_slice(&self.to_le_bytes());
        result
    }
}

impl IntoOrigin for sp_runtime::AccountId32 {
    fn into_origin(self) -> H256 {
        H256::from(self.as_ref())
    }
}

impl IntoOrigin for H256 {
    fn into_origin(self) -> H256 {
        self
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum MessageOrigin {
    External(H256),
    Internal(H256),
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MessageRoute {
    pub origin: MessageOrigin,
    pub destination: H256,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum IntermediateMessage {
    InitProgram { external_origin: H256, program_id: H256, code: Vec<u8>, payload: Vec<u8>, gas_limit: u64 },
    DispatchMessage { route: MessageRoute, payload: Vec<u8>, gas_limit: u64 },
}

fn program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::prog::");
    id.encode_to(&mut key);
    key
}

fn page_key(page: u32) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::alloc::");
    page.encode_to(&mut key);
    key
}

pub fn get_program(id: H256) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

pub fn set_program(id: H256, program: Program) {
    sp_io::storage::set(
        &program_key(id),
        &program.encode(),
    )
}

pub fn remove_program(_id: H256) {
    unimplemented!()
}

pub fn dequeue_message() -> Option<Message> {
    sp_io::storage::get(b"g::msg")
        .map(|val| Vec::<Message>::decode(&mut &val[..]).expect("values encoded correctly"))
        .and_then(|mut messages| {
            let popped =
                if messages.len() > 0 { Some(messages.remove(0)) }
                else { None };
            sp_io::storage::set(b"g::msg", &messages.encode());
            popped
        })
}

pub fn queue_message(message: Message) {
    let mut messages = sp_io::storage::get(b"g::msg")
        .map(|val| Vec::<Message>::decode(&mut &val[..]).expect("values encoded correctly"))
        .unwrap_or_default();

    messages.push(message);

    sp_io::storage::set(b"g::msg", &messages.encode());
}

pub fn alloc(page: u32, program: H256) {
    sp_io::storage::set(
        &page_key(page),
        &program.encode(),
    )
}

pub fn page_info(page: u32) -> Option<H256> {
    sp_io::storage::get(&page_key(page))
        .map(|val| H256::decode(&mut &val[..]).expect("values encoded correctly"))
}

pub fn dealloc(page: u32) {
    sp_io::storage::clear(&page_key(page))
}
