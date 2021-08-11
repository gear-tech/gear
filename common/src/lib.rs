// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod native;
pub mod storage_queue;

use codec::{Decode, Encode};
use sp_core::{crypto::UncheckedFrom, H256};
use sp_std::collections::btree_map::BTreeMap;
use sp_std::prelude::*;

use storage_queue::StorageQueue;

pub type ExitCode = i32;

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Message {
    pub id: H256,
    pub source: H256,
    pub dest: H256,
    pub payload: Vec<u8>,
    pub gas_limit: u64,
    pub value: u128,
    pub reply: Option<(H256, ExitCode)>,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub struct Program {
    pub static_pages: u32,
    pub persistent_pages: BTreeMap<u32, Vec<u8>>,
    pub code_hash: H256,
    pub nonce: u64,
}

pub trait Origin: Sized {
    fn into_origin(self) -> H256;
    fn from_origin(val: H256) -> Self;
}

impl Origin for u64 {
    fn into_origin(self) -> H256 {
        let mut result = H256::zero();
        result[0..8].copy_from_slice(&self.to_le_bytes());
        result
    }

    fn from_origin(v: H256) -> Self {
        // h256 -> u64 should not be used anywhere other than in tests!
        let mut val = [0u8; 8];
        val.copy_from_slice(&v[0..8]);
        Self::from_le_bytes(val)
    }
}

impl Origin for sp_runtime::AccountId32 {
    fn into_origin(self) -> H256 {
        H256::from(self.as_ref())
    }

    fn from_origin(v: H256) -> Self {
        sp_runtime::AccountId32::unchecked_from(v)
    }
}

impl Origin for H256 {
    fn into_origin(self) -> H256 {
        self
    }

    fn from_origin(val: H256) -> Self {
        val
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum IntermediateMessage {
    InitProgram {
        origin: H256,
        program_id: H256,
        code: Vec<u8>,
        init_message_id: H256,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    },
    DispatchMessage {
        id: H256,
        origin: H256,
        destination: H256,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        reply: Option<H256>,
    },
}

fn program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::prog::");
    id.encode_to(&mut key);
    key
}

fn code_key(code_hash: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::code::");
    code_hash.encode_to(&mut key);
    key
}

fn page_key(page: u32) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::alloc::");
    page.encode_to(&mut key);
    key
}

fn wait_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(b"g::wait::");
    id.encode_to(&mut key);
    key
}

pub fn get_code(code_hash: H256) -> Option<Vec<u8>> {
    sp_io::storage::get(&code_key(code_hash))
}

pub fn set_code(code_hash: H256, code: &[u8]) {
    sp_io::storage::set(&code_key(code_hash), code)
}

pub fn get_program(id: H256) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

pub fn set_program(id: H256, program: Program) {
    sp_io::storage::set(&program_key(id), &program.encode())
}

pub fn remove_program(id: H256) {
    // TODO: remove the corresponding code, if it's not referenced by any other program (#126)
    sp_io::storage::clear(&program_key(id))
}

pub fn program_exists(id: H256) -> bool {
    sp_io::storage::exists(&program_key(id))
}

pub fn dequeue_message() -> Option<Message> {
    let mut message_queue = StorageQueue::get(b"g::msg::".as_ref());
    message_queue.dequeue()
}

pub fn queue_message(message: Message) {
    let mut message_queue = StorageQueue::get(b"g::msg::".as_ref());
    let id = message.id.clone();
    message_queue.queue(message, id);
}

pub fn alloc(page: u32, program: H256) {
    sp_io::storage::set(&page_key(page), &program.encode())
}

pub fn page_info(page: u32) -> Option<H256> {
    sp_io::storage::get(&page_key(page))
        .map(|val| H256::decode(&mut &val[..]).expect("values encoded correctly"))
}

pub fn dealloc(page: u32) {
    sp_io::storage::clear(&page_key(page))
}

pub fn nonce_fetch_inc() -> u128 {
    let original_nonce = sp_io::storage::get(b"g::msg::nonce")
        .map(|val| u128::decode(&mut &val[..]).expect("nonce decode fail"))
        .unwrap_or(0u128);

    let new_nonce = original_nonce.wrapping_add(1);

    sp_io::storage::set(b"g::msg::nonce", &new_nonce.encode());

    original_nonce
}

// WARN: Never call that in threads
pub fn next_message_id(payload: &Vec<u8>) -> H256 {
    let nonce = nonce_fetch_inc();
    let mut message_id = payload.encode();
    message_id.extend_from_slice(&nonce.to_le_bytes());
    let message_id: H256 = sp_io::hashing::blake2_256(&message_id).into();
    message_id
}

pub fn caller_nonce_fetch_inc(caller_id: H256) -> u64 {
    let mut key_id = b"g::msg::user_nonce".to_vec();
    key_id.extend(&caller_id[..]);

    let original_nonce = sp_io::storage::get(&key_id)
        .map(|val| u64::decode(&mut &val[..]).expect("nonce decode fail"))
        .unwrap_or(0);

    let new_nonce = original_nonce.wrapping_add(1);

    sp_io::storage::set(&key_id, &new_nonce.encode());

    original_nonce
}

pub(crate) fn insert_waiting_message(id: H256, message: Message) {
    sp_io::storage::set(&wait_key(id), &message.encode());
}

pub(crate) fn get_waiting_message(id: H256) -> Option<Message> {
    sp_io::storage::get(&wait_key(id))
        .as_ref()
        .map(|val| Message::decode(&mut &val[..]).ok())
        .flatten()
}

pub(crate) fn remove_waiting_message(id: H256) -> Option<Message> {
    let id = wait_key(id);
    let msg: Option<Message> = sp_io::storage::get(&id)
        .map(|val| Message::decode(&mut &val[..]).expect("message encoded correctly"));

    if msg.is_some() {
        sp_io::storage::clear(&id);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_incremented() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            assert_eq!(nonce_fetch_inc(), 0_u128);
            assert_eq!(nonce_fetch_inc(), 1_u128);
            assert_eq!(nonce_fetch_inc(), 2_u128);
        });
    }

    #[test]
    fn program_decoded() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let code = b"pretended wasm code".to_vec();
            let code_hash: H256 = sp_io::hashing::blake2_256(&code[..]).into();
            let program_id = H256::from_low_u64_be(1);
            let program = Program {
                static_pages: 256,
                persistent_pages: Default::default(),
                code_hash,
                nonce: 0,
            };
            set_code(code_hash, &code);
            assert!(get_program(program_id).is_none());
            set_program(program_id, program.clone());
            assert_eq!(get_program(program_id).unwrap(), program);
        });
    }
}
