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

pub const STORAGE_PROGRAM_PREFIX: &'static [u8] = b"g::prog::";
pub const STORAGE_MESSAGE_PREFIX: &'static [u8] = b"g::msg::";
pub const STORAGE_MESSAGE_NONCE_KEY: &'static [u8] = b"g::msg::nonce";
pub const STORAGE_MESSAGE_USER_NONCE_KEY: &'static [u8] = b"g::msg::user_nonce";
pub const STORAGE_CODE_PREFIX: &'static [u8] = b"g::code::";
pub const STORAGE_CODE_REFS_PREFIX: &'static [u8] = b"g::code::refs";
pub const STORAGE_WAITLIST_PREFIX: &'static [u8] = b"g::wait::";
pub const STORAGE_ALLOCATION_PREFIX: &'static [u8] = b"g::alloc::";

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
    key.extend(STORAGE_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

fn code_key(code_hash: H256) -> (Vec<u8>, Vec<u8>) {
    let (mut key, mut ref_counter) = (Vec::new(), Vec::new());
    key.extend(STORAGE_CODE_PREFIX);
    code_hash.encode_to(&mut key);
    ref_counter.extend(STORAGE_CODE_REFS_PREFIX);
    code_hash.encode_to(&mut ref_counter);
    (key, ref_counter)
}

fn page_key(page: u32) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_ALLOCATION_PREFIX);
    page.encode_to(&mut key);
    key
}

fn wait_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn get_code(code_hash: H256) -> Option<Vec<u8>> {
    sp_io::storage::get(&code_key(code_hash).0)
}

fn set_code(code_hash: H256, code: &[u8]) {
    sp_io::storage::set(&code_key(code_hash).0, code)
}

fn get_code_refs(code_hash: H256) -> u32 {
    sp_io::storage::get(&code_key(code_hash).1)
        .map(|val| {
            let mut v = [0u8; 4];
            if val.len() == 4 {
                v.copy_from_slice(&val[0..4]);
            }
            u32::from_le_bytes(v)
        })
        .unwrap_or_default()
}

fn set_code_refs(code_hash: H256, value: u32) {
    sp_io::storage::set(&code_key(code_hash).1, &value.to_le_bytes())
}

fn add_code_ref(code_hash: H256) {
    set_code_refs(code_hash, get_code_refs(code_hash).saturating_add(1))
}

fn release_code(code_hash: H256) {
    let new_refs = get_code_refs(code_hash).saturating_sub(1);
    if new_refs == 0 {
        // Clearing storage for both code itself and its reference counter
        sp_io::storage::clear(&code_key(code_hash).1);
        sp_io::storage::clear(&code_key(code_hash).0);
        return;
    }
    set_code_refs(code_hash, new_refs)
}

pub fn get_program(id: H256) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

pub fn set_program(id: H256, program: Program) {
    if !program_exists(id) {
        add_code_ref(program.code_hash);
    }
    sp_io::storage::set(&program_key(id), &program.encode())
}

pub fn remove_program(id: H256) {
    if let Some(program) = get_program(id) {
        release_code(program.code_hash);
    }
    sp_io::storage::clear(&program_key(id))
}

pub fn program_exists(id: H256) -> bool {
    sp_io::storage::exists(&program_key(id))
}

pub fn dequeue_message() -> Option<Message> {
    let mut message_queue = StorageQueue::get(STORAGE_MESSAGE_PREFIX);
    message_queue.dequeue()
}

pub fn queue_message(message: Message) {
    let mut message_queue = StorageQueue::get(STORAGE_MESSAGE_PREFIX);
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
    let original_nonce = sp_io::storage::get(STORAGE_MESSAGE_NONCE_KEY)
        .map(|val| u128::decode(&mut &val[..]).expect("nonce decode fail"))
        .unwrap_or(0u128);

    let new_nonce = original_nonce.wrapping_add(1);

    sp_io::storage::set(STORAGE_MESSAGE_NONCE_KEY, &new_nonce.encode());

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
    let mut key_id = STORAGE_MESSAGE_USER_NONCE_KEY.to_vec();
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
            assert_eq!(get_code(program.code_hash).unwrap(), code);
        });
    }

    #[test]
    fn unused_code_removal_works() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let code = b"pretended wasm code".to_vec();
            let code_hash: H256 = sp_io::hashing::blake2_256(&code[..]).into();
            set_code(code_hash, &code);

            // At first no program references the code
            assert_eq!(get_code_refs(code_hash), 0u32);

            set_program(
                H256::from_low_u64_be(1),
                Program {
                    static_pages: 256,
                    persistent_pages: Default::default(),
                    code_hash,
                    nonce: 0,
                },
            );
            assert_eq!(get_code_refs(code_hash), 1u32);

            set_program(
                H256::from_low_u64_be(2),
                Program {
                    static_pages: 128,
                    persistent_pages: Default::default(),
                    code_hash,
                    nonce: 1,
                },
            );
            assert_eq!(get_code_refs(code_hash), 2u32);

            remove_program(H256::from_low_u64_be(1));
            assert_eq!(get_code_refs(code_hash), 1u32);

            assert!(get_code(code_hash).is_some());

            remove_program(H256::from_low_u64_be(2));
            assert_eq!(get_code_refs(code_hash), 0u32);

            assert!(get_code(code_hash).is_none());
        });
    }
}
