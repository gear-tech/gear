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

pub mod native;
pub mod storage_queue;
pub mod value_tree;

use codec::{Decode, Encode};
use frame_support::{
    dispatch::DispatchError,
    weights::{IdentityFee, WeightToFeePolynomial},
};
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::crypto::UncheckedFrom;
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};
use sp_std::prelude::*;
use gear_runtime_interface as gear_ri;

pub use storage_queue::Iterator;
use storage_queue::StorageQueue;

pub const STORAGE_PROGRAM_PREFIX: &[u8] = b"g::prog::";
pub const STORAGE_PROGRAM_PAGES_PREFIX: &[u8] = b"g::pages::";
pub const STORAGE_PROGRAM_STATE_WAIT_PREFIX: &[u8] = b"g::prog_wait::";
pub const STORAGE_MESSAGE_PREFIX: &[u8] = b"g::msg::";
pub const STORAGE_MESSAGE_NONCE_KEY: &[u8] = b"g::msg::nonce";
pub const STORAGE_MESSAGE_USER_NONCE_KEY: &[u8] = b"g::msg::user_nonce";
pub const STORAGE_CODE_PREFIX: &[u8] = b"g::code::";
pub const STORAGE_CODE_METADATA_PREFIX: &[u8] = b"g::code::metadata::";
pub const STORAGE_CODE_REFS_PREFIX: &[u8] = b"g::code::refs::";
pub const STORAGE_WAITLIST_PREFIX: &[u8] = b"g::wait::";

pub const GAS_VALUE_PREFIX: &[u8] = b"g::gas_tree";

pub type ExitCode = i32;

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Message {
    pub id: H256,
    pub source: H256,
    pub dest: H256,
    pub payload: Vec<u8>,
    pub gas_limit: u64,
    pub value: u128,
    pub reply: Option<(H256, ExitCode)>,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct Program {
    pub static_pages: u32,
    pub persistent_pages: BTreeSet<u32>,
    pub code_hash: H256,
    pub nonce: u64,
    pub state: ProgramState,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct CodeMetadata {
    pub author: H256,
    pub block_number: u32,
}

impl CodeMetadata {
    pub fn new(author: H256, block_number: u32) -> Self {
        CodeMetadata {
            author,
            block_number,
        }
    }
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

pub trait GasToFeeConverter {
    type Balance: BaseArithmetic + From<u32> + Copy + Unsigned;

    fn gas_to_fee(gas: u64) -> Self::Balance {
        IdentityFee::<Self::Balance>::calc(&gas)
    }
}

pub trait PaymentProvider<AccountId> {
    type Balance;

    fn withhold_reserved(
        source: H256,
        dest: &AccountId,
        amount: Self::Balance,
    ) -> Result<(), DispatchError>;
}

// Inner enum used to "generalise" get/set of data under "g::code::*" prefixes
enum CodeKeyPrefixKind {
    // "g::code::"
    RawCode,
    // "g::code::refs::"
    CodeRef,
    // "g::code::metadata::"
    CodeMetadata,
}

fn program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

fn code_key(code_hash: H256, kind: CodeKeyPrefixKind) -> Vec<u8> {
    let prefix = match kind {
        CodeKeyPrefixKind::RawCode => STORAGE_CODE_PREFIX,
        CodeKeyPrefixKind::CodeRef => STORAGE_CODE_REFS_PREFIX,
        CodeKeyPrefixKind::CodeMetadata => STORAGE_CODE_METADATA_PREFIX,
    };
    // key's length is N bytes of code hash + M bytes of prefix
    // currently code hash is 32 bytes
    let mut key = Vec::with_capacity(prefix.len() + code_hash.as_bytes().len());
    key.extend(prefix);
    code_hash.encode_to(&mut key);
    key
}

fn page_key(id: H256, page: u32) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    id.encode_to(&mut key);
    key.extend(b"::");
    page.encode_to(&mut key);
    key
}

pub fn wait_key(prog_id: H256, msg_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    prog_id.encode_to(&mut key);
    key.extend(b"::");
    msg_id.encode_to(&mut key);

    key
}

pub fn get_code(code_hash: H256) -> Option<Vec<u8>> {
    sp_io::storage::get(&code_key(code_hash, CodeKeyPrefixKind::RawCode))
}

pub fn set_code(code_hash: H256, code: &[u8]) {
    sp_io::storage::set(&code_key(code_hash, CodeKeyPrefixKind::RawCode), code)
}

pub fn set_code_metadata(code_hash: H256, metadata: CodeMetadata) {
    sp_io::storage::set(
        &code_key(code_hash, CodeKeyPrefixKind::CodeMetadata),
        &metadata.encode(),
    )
}

pub fn get_code_metadata(code_hash: H256) -> Option<CodeMetadata> {
    sp_io::storage::get(&code_key(code_hash, CodeKeyPrefixKind::CodeMetadata))
        .map(|data| CodeMetadata::decode(&mut &data[..]).expect("data encoded correctly"))
}

/// Enumeration contains variants for program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub enum ProgramState {
    /// `init` method of a program has not yet finished its execution so
    /// the program is not considered as initialized. All messages to such a
    /// program go to the wait list.
    /// `message_id` contains identifier of the initialization message.
    Uninitialized { message_id: H256 },
    /// Program has been successfully initialized and can process messages.
    Initialized,
}

pub fn get_program_state(id: H256) -> Option<ProgramState> {
    get_program(id).map(|p| p.state)
}

pub fn set_program_initialized(id: H256) {
    if let Some(mut p) = get_program(id) {
        if !matches!(p.state, ProgramState::Initialized) {
            p.state = ProgramState::Initialized;
            sp_io::storage::set(&program_key(id), &p.encode());
        }
    }
}

fn get_code_refs(code_hash: H256) -> u32 {
    sp_io::storage::get(&code_key(code_hash, CodeKeyPrefixKind::CodeRef))
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
    sp_io::storage::set(
        &code_key(code_hash, CodeKeyPrefixKind::CodeRef),
        &value.to_le_bytes(),
    )
}

fn add_code_ref(code_hash: H256) {
    set_code_refs(code_hash, get_code_refs(code_hash).saturating_add(1))
}

fn release_code(code_hash: H256) {
    let new_refs = get_code_refs(code_hash).saturating_sub(1);
    if new_refs == 0 {
        // Clearing storage for both code itself and its reference counter
        sp_io::storage::clear(&code_key(code_hash, CodeKeyPrefixKind::CodeRef));
        sp_io::storage::clear(&code_key(code_hash, CodeKeyPrefixKind::RawCode));
        return;
    }
    set_code_refs(code_hash, new_refs)
}

pub fn get_program(id: H256) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

/// Returns mem page data from storage for program `id` and `page_idx`
pub fn get_program_page_data(id: H256, page_idx: u32) -> Option<Vec<u8>> {
    let key = page_key(id, page_idx);
    sp_io::storage::get(&key)
}

/// Save page data key in storage
pub fn save_page_lazy_info(id: H256, page_num: u32) {
    let key = page_key(id, page_num);
    gear_ri::gear_ri::save_page_lazy_info(page_num, &key);
}

pub fn get_program_pages(id: H256, pages: BTreeSet<u32>) -> BTreeMap<u32, Vec<u8>> {
    let mut persistent_pages = BTreeMap::new();
    for page_num in pages {
        let key = page_key(id, page_num);

        persistent_pages.insert(
            page_num,
            sp_io::storage::get(&key).expect("values encoded correctly"),
        );
    }
    persistent_pages
}

pub fn set_program(id: H256, program: Program, persistent_pages: BTreeMap<u32, Vec<u8>>) {
    if !program_exists(id) {
        add_code_ref(program.code_hash);
    }
    for (page_num, page_buf) in persistent_pages {
        let key = page_key(id, page_num);
        sp_io::storage::set(&key, &page_buf);
    }
    sp_io::storage::set(&program_key(id), &program.encode())
}

pub fn remove_program(id: H256) {
    if let Some(program) = get_program(id) {
        release_code(program.code_hash);
    }
    let mut pages_prefix = STORAGE_PROGRAM_PAGES_PREFIX.to_vec();
    pages_prefix.extend(&program_key(id));
    sp_io::storage::clear_prefix(&pages_prefix, None);
    sp_io::storage::clear_prefix(&program_key(id), None);
    sp_io::storage::clear_prefix(&waiting_init_prefix(id), None);
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
    let id = message.id;
    message_queue.queue(message, id);
}

pub fn message_iter() -> Iterator<Message> {
    StorageQueue::get(STORAGE_MESSAGE_PREFIX).into_iter()
}

pub fn nonce_fetch_inc() -> u128 {
    let original_nonce = sp_io::storage::get(STORAGE_MESSAGE_NONCE_KEY)
        .map(|val| u128::decode(&mut &val[..]).expect("nonce decode fail"))
        .unwrap_or(0u128);

    let new_nonce = original_nonce.wrapping_add(1);

    sp_io::storage::set(STORAGE_MESSAGE_NONCE_KEY, &new_nonce.encode());

    original_nonce
}

pub fn peek_last_message_id(payload: &[u8]) -> H256 {
    let nonce = sp_io::storage::get(STORAGE_MESSAGE_NONCE_KEY)
        .map(|val| u128::decode(&mut &val[..]).expect("nonce decode fail"))
        .unwrap_or(0u128);

    let mut data = payload.encode();
    data.extend_from_slice(&(nonce.wrapping_sub(1)).to_le_bytes());
    let message_id: H256 = sp_io::hashing::blake2_256(&data).into();
    message_id
}

// WARN: Never call that in threads
pub fn next_message_id(payload: &[u8]) -> H256 {
    let nonce = nonce_fetch_inc();
    let mut data = payload.encode();
    data.extend_from_slice(&nonce.to_le_bytes());
    let message_id: H256 = sp_io::hashing::blake2_256(&data).into();
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

pub fn set_program_nonce(id: H256, nonce: u64) {
    if let Some(mut prog) = sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
    {
        prog.nonce = nonce;

        sp_io::storage::set(&program_key(id), &prog.encode())
    }
}

pub fn set_program_persistent_pages(id: H256, persistent_pages: BTreeSet<u32>) {
    if let Some(mut prog) = sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
    {
        prog.persistent_pages = persistent_pages;

        sp_io::storage::set(&program_key(id), &prog.encode())
    }
}

pub fn set_program_page(program_id: H256, page_num: u32, page_buf: Vec<u8>) {
    let page_key = page_key(program_id, page_num);

    sp_io::storage::set(&page_key, &page_buf);
}

pub fn remove_program_page(program_id: H256, page_num: u32) {
    let page_key = page_key(program_id, page_num);

    sp_io::storage::clear(&page_key);
}

pub fn insert_waiting_message(dest_prog_id: H256, msg_id: H256, message: Message, bn: u32) {
    let payload = (message, bn);
    sp_io::storage::set(&wait_key(dest_prog_id, msg_id), &payload.encode());
}

pub fn remove_waiting_message(dest_prog_id: H256, msg_id: H256) -> Option<(Message, u32)> {
    let id = wait_key(dest_prog_id, msg_id);
    let msg = sp_io::storage::get(&id).and_then(|val| <(Message, u32)>::decode(&mut &val[..]).ok());

    if msg.is_some() {
        sp_io::storage::clear(&id);
    }
    msg
}

fn waiting_init_prefix(prog_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_STATE_WAIT_PREFIX);
    prog_id.encode_to(&mut key);

    key
}

pub fn waiting_init_append_message_id(dest_prog_id: H256, message_id: H256) {
    let key = waiting_init_prefix(dest_prog_id);
    sp_io::storage::append(&key, message_id.encode());
}

pub fn waiting_init_take_messages(dest_prog_id: H256) -> Vec<H256> {
    let key = waiting_init_prefix(dest_prog_id);
    let messages = sp_io::storage::get(&key).and_then(|v| Vec::<H256>::decode(&mut &v[..]).ok());
    sp_io::storage::clear(&key);

    messages.unwrap_or_default()
}

pub fn code_exists(code_hash: H256) -> bool {
    sp_io::storage::exists(&code_key(code_hash, CodeKeyPrefixKind::RawCode))
}

pub fn reset_storage() {
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PAGES_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_CODE_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_WAITLIST_PREFIX, None);
    sp_io::storage::clear_prefix(GAS_VALUE_PREFIX, None);
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
                state: ProgramState::Initialized,
            };
            set_code(code_hash, &code);
            assert!(get_program(program_id).is_none());
            set_program(program_id, program.clone(), Default::default());
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
                    state: ProgramState::Initialized,
                },
                Default::default(),
            );
            assert_eq!(get_code_refs(code_hash), 1u32);

            set_program(
                H256::from_low_u64_be(2),
                Program {
                    static_pages: 128,
                    persistent_pages: Default::default(),
                    code_hash,
                    nonce: 1,
                    state: ProgramState::Initialized,
                },
                Default::default(),
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
