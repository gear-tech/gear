// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

pub mod lazy_pages;
pub mod storage_queue;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use codec::{Decode, Encode};
use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    traits::Imbalance,
    weights::{IdentityFee, WeightToFeePolynomial},
};
use gear_core::{
    identifiers::{CodeId, MessageId, ProgramId},
    message::StoredDispatch,
    program::Program as NativeProgram,
};
use gear_runtime_interface as gear_ri;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

pub use storage_queue::{Iterator, StorageQueue};

pub const STORAGE_PROGRAM_PREFIX: &[u8] = b"g::prog::";
pub const STORAGE_PROGRAM_PAGES_PREFIX: &[u8] = b"g::pages::";
pub const STORAGE_PROGRAM_STATE_WAIT_PREFIX: &[u8] = b"g::prog_wait::";
pub const STORAGE_MESSAGE_PREFIX: &[u8] = b"g::msg::";
pub const STORAGE_MESSAGE_USER_NONCE_KEY: &[u8] = b"g::msg::user_nonce";
pub const STORAGE_CODE_PREFIX: &[u8] = b"g::code::";
pub const STORAGE_CODE_METADATA_PREFIX: &[u8] = b"g::code::metadata::";
pub const STORAGE_WAITLIST_PREFIX: &[u8] = b"g::wait::";
pub const GAS_VALUE_PREFIX: &[u8] = b"g::gas_tree";

pub trait NativeAddress: Sized {
    fn into_native(self) -> ProgramId;
    fn from_native(val: ProgramId) -> Self;
}

impl NativeAddress for sp_runtime::AccountId32 {
    fn into_native(self) -> ProgramId {
        let bytes: [u8; 32] = self.into();
        bytes.into()
    }

    fn from_native(v: ProgramId) -> Self {
        sp_runtime::AccountId32::new(v.into())
    }
}

pub trait GasPrice {
    type Balance: BaseArithmetic + From<u32> + Copy + Unsigned;

    /// A price for the `gas` amount of gas.
    /// In general case, this doesn't necessarily has to be constant.
    fn gas_price(gas: u64) -> Self::Balance {
        IdentityFee::<Self::Balance>::calc(&gas)
    }
}

pub trait PaymentProvider<AccountId> {
    type Balance;

    fn withhold_reserved(
        source: ProgramId,
        dest: &AccountId,
        amount: Self::Balance,
    ) -> Result<(), DispatchError>;
}

/// Abstraction for a chain of value items each piece of which has an attributed owner and
/// can be traced up to some root origin.
/// The definition is largely inspired by the `frame_support::traits::Currency` -
/// https://github.com/paritytech/substrate/blob/master/frame/support/src/traits/tokens/currency.rs,
/// however, the intended use is very close to the UTxO based ledger model.
pub trait DAGBasedLedger {
    /// Type representing the external owner of a value (gas) item.
    type ExternalOrigin;

    /// Type that identifies a particular value item.
    type Key;

    /// Type representing a quantity of value.
    type Balance;

    /// Types to denote a result of some unbalancing operation - that is operations that create
    /// inequality between the underlying value supply and some hypothetical "collateral" asset.

    /// `PositiveImbalance` indicates that some value has been created, which will eventually
    /// lead to an increase in total supply.
    type PositiveImbalance: Imbalance<Self::Balance, Opposite = Self::NegativeImbalance>;

    /// `NegativeImbalance` indicates that some value has been removed from circulation
    /// leading to a decrease in the total supply of the underlying value.
    type NegativeImbalance: Imbalance<Self::Balance, Opposite = Self::PositiveImbalance>;

    /// The total amount of value currently in circulation.
    fn total_supply() -> Self::Balance;

    /// Increase the total issuance of the underlying value by creating some `amount` of it
    /// and attributing it to the `origin`. The `key` identifies the created "bag" of value.
    /// In case the `key` already indentifies some other piece of value an error is returned.
    fn create(
        origin: Self::ExternalOrigin,
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::PositiveImbalance, DispatchError>;

    /// Get value item by it's ID, if exists.
    fn get_limit(key: Self::Key) -> Option<(Self::Balance, Self::ExternalOrigin)>;

    /// Consume underlying value.
    ///
    /// If `key` does not identify any value or the value can't be fully consumed due to
    /// being a part of other value or itself having unconsumed parts, return None,
    /// else the corresponding piece of value is destroyed and imbalance is created.
    fn consume(key: Self::Key) -> Option<(Self::NegativeImbalance, Self::ExternalOrigin)>;

    /// Burns underlying value.
    ///
    /// This "spends" the specified amount of value thereby decreasing the overall supply of it.
    /// In case of a success, this indicates the entire value supply becomes over-collateralized,
    /// hence negative imbalance.
    fn spend(
        key: Self::Key,
        amount: Self::Balance,
    ) -> Result<Self::NegativeImbalance, DispatchError>;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value or the `amount` exceeds what's locked under that key,
    /// an error is returned.
    /// This can't create imbalance as no value is burned or created.
    fn split_with_value(
        key: Self::Key,
        new_key: Self::Key,
        amount: Self::Balance,
    ) -> DispatchResult;

    /// Split underlying value.
    ///
    /// If `key` does not identify any value an error is returned.
    /// This can't create imbalance as no value is burned or created.
    fn split(key: Self::Key, new_key: Self::Key) -> DispatchResult;
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub enum Program {
    Active(ActiveProgram),
    Terminated,
}

#[derive(Clone, Copy, Debug)]
pub enum ProgramError {
    CodeHashNotFound,
    IsTerminated,
    DoesNotExist,
}

impl Program {
    pub fn try_into_native(self, id: ProgramId) -> Result<NativeProgram, ProgramError> {
        let is_initialized = self.is_initialized();
        let program: ActiveProgram = self.try_into()?;
        let code = crate::get_code(program.code_id).ok_or(ProgramError::CodeHashNotFound)?;
        let native_program = NativeProgram::from_parts(
            id,
            code,
            program.static_pages,
            program.persistent_pages,
            is_initialized,
        );
        Ok(native_program)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Program::Active(_))
    }

    pub fn is_terminated(&self) -> bool {
        matches!(self, Program::Terminated)
    }

    pub fn is_initialized(&self) -> bool {
        matches!(
            self,
            Program::Active(ActiveProgram {
                state: ProgramState::Initialized,
                ..
            })
        )
    }

    pub fn is_uninitialized(&self) -> bool {
        matches!(
            self,
            Program::Active(ActiveProgram {
                state: ProgramState::Uninitialized { .. },
                ..
            })
        )
    }
}

impl TryFrom<Program> for ActiveProgram {
    type Error = ProgramError;

    fn try_from(prog_with_status: Program) -> Result<ActiveProgram, Self::Error> {
        match prog_with_status {
            Program::Active(p) => Ok(p),
            Program::Terminated => Err(ProgramError::IsTerminated),
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct ActiveProgram {
    pub static_pages: u32,
    pub persistent_pages: BTreeSet<u32>,
    pub code_id: CodeId,
    pub state: ProgramState,
}

/// Enumeration contains variants for program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub enum ProgramState {
    /// `init` method of a program has not yet finished its execution so
    /// the program is not considered as initialized. All messages to such a
    /// program go to the wait list.
    /// `message_id` contains identifier of the initialization message.
    Uninitialized { message_id: MessageId },
    /// Program has been successfully initialized and can process messages.
    Initialized,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct CodeMetadata {
    pub author: ProgramId,
    pub block_number: u32,
}

impl CodeMetadata {
    pub fn new(author: ProgramId, block_number: u32) -> Self {
        CodeMetadata {
            author,
            block_number,
        }
    }
}

// Inner enum used to "generalise" get/set of data under "g::code::*" prefixes
enum CodeKeyPrefixKind {
    // "g::code::"
    RawCode,
    // "g::code::metadata::"
    CodeMetadata,
}

pub fn program_key(id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PREFIX);
    key.extend_from_slice(id.as_ref());
    key
}

fn code_key(code_id: CodeId, kind: CodeKeyPrefixKind) -> Vec<u8> {
    let prefix = match kind {
        CodeKeyPrefixKind::RawCode => STORAGE_CODE_PREFIX,
        CodeKeyPrefixKind::CodeMetadata => STORAGE_CODE_METADATA_PREFIX,
    };
    // key's length is N bytes of code hash + M bytes of prefix
    // currently code hash is 32 bytes
    let mut key = Vec::with_capacity(prefix.len() + code_id.as_ref().len());
    key.extend(prefix);
    code_id.encode_to(&mut key);
    key
}

pub fn pages_prefix(program_id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    key.extend_from_slice(program_id.as_ref());

    key
}

fn page_key(id: ProgramId, page: u32) -> Vec<u8> {
    let mut key = pages_prefix(id);
    key.extend(b"::");
    key.extend_from_slice(&page.to_le_bytes());

    key
}

pub fn wait_prefix(prog_id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    key.extend_from_slice(prog_id.as_ref());
    key.extend(b"::");
    key
}

pub fn wait_key(prog_id: ProgramId, msg_id: MessageId) -> Vec<u8> {
    let mut key = wait_prefix(prog_id);
    key.extend_from_slice(msg_id.as_ref());
    key
}

pub fn get_code(code_id: CodeId) -> Option<Vec<u8>> {
    sp_io::storage::get(&code_key(code_id, CodeKeyPrefixKind::RawCode))
}

pub fn set_code(code_id: CodeId, code: &[u8]) {
    sp_io::storage::set(&code_key(code_id, CodeKeyPrefixKind::RawCode), code)
}

pub fn set_code_metadata(code_id: CodeId, metadata: CodeMetadata) {
    sp_io::storage::set(
        &code_key(code_id, CodeKeyPrefixKind::CodeMetadata),
        &metadata.encode(),
    )
}

pub fn get_code_metadata(code_id: CodeId) -> Option<CodeMetadata> {
    sp_io::storage::get(&code_key(code_id, CodeKeyPrefixKind::CodeMetadata))
        .map(|data| CodeMetadata::decode(&mut &data[..]).expect("data encoded correctly"))
}

pub fn set_program_initialized(id: ProgramId) {
    if let Some(Program::Active(mut p)) = get_program(id) {
        if !matches!(p.state, ProgramState::Initialized) {
            p.state = ProgramState::Initialized;
            sp_io::storage::set(&program_key(id), &Program::Active(p).encode());
        }
    }
}

pub fn set_program_terminated_status(id: ProgramId) -> Result<(), ProgramError> {
    if let Some(program) = get_program(id) {
        if program.is_terminated() {
            return Err(ProgramError::IsTerminated);
        }

        sp_io::storage::clear_prefix(&pages_prefix(id), None);
        sp_io::storage::set(&program_key(id), &Program::Terminated.encode());

        Ok(())
    } else {
        Err(ProgramError::DoesNotExist)
    }
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

/// Returns mem page data from storage for program `id` and `page_idx`
pub fn get_program_page_data(id: ProgramId, page_idx: u32) -> Option<Vec<u8>> {
    let key = page_key(id, page_idx);
    sp_io::storage::get(&key)
}

/// Save page data key in storage
pub fn save_page_lazy_info(id: ProgramId, page_num: u32) {
    let key = page_key(id, page_num);
    gear_ri::gear_ri::save_page_lazy_info(page_num, &key);
}

pub fn get_program_pages(id: ProgramId, pages: BTreeSet<u32>) -> Option<BTreeMap<u32, Vec<u8>>> {
    let mut persistent_pages = BTreeMap::new();
    for page_num in pages {
        let key = page_key(id, page_num);

        persistent_pages.insert(page_num, sp_io::storage::get(&key)?);
    }
    Some(persistent_pages)
}

pub fn set_program(
    id: ProgramId,
    program: ActiveProgram,
    persistent_pages: BTreeMap<u32, Vec<u8>>,
) {
    for (page_num, page_buf) in persistent_pages {
        let key = page_key(id, page_num);
        sp_io::storage::set(&key, &page_buf);
    }
    sp_io::storage::set(&program_key(id), &Program::Active(program).encode())
}

pub fn program_exists(id: ProgramId) -> bool {
    sp_io::storage::exists(&program_key(id))
}

pub fn clear_dispatch_queue() {
    sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
}

pub fn dequeue_dispatch() -> Option<StoredDispatch> {
    let mut dispatch_queue = StorageQueue::<MessageId, StoredDispatch>::get(STORAGE_MESSAGE_PREFIX);
    dispatch_queue.dequeue()
}

pub fn queue_dispatch(dispatch: StoredDispatch) {
    let mut dispatch_queue = StorageQueue::<MessageId, StoredDispatch>::get(STORAGE_MESSAGE_PREFIX);
    dispatch_queue.queue(dispatch.id(), dispatch);
}

pub fn dispatch_iter() -> Iterator<MessageId, StoredDispatch> {
    StorageQueue::<MessageId, StoredDispatch>::get(STORAGE_MESSAGE_PREFIX).into_iter()
}

pub fn set_program_persistent_pages(id: ProgramId, persistent_pages: BTreeSet<u32>) {
    if let Some(Program::Active(mut prog)) = get_program(id) {
        prog.persistent_pages = persistent_pages;
        sp_io::storage::set(&program_key(id), &Program::Active(prog).encode())
    }
}

pub fn set_program_page(program_id: ProgramId, page_num: u32, page_buf: Vec<u8>) {
    let page_key = page_key(program_id, page_num);

    sp_io::storage::set(&page_key, &page_buf);
}

pub fn remove_program_page(program_id: ProgramId, page_num: u32) {
    let page_key = page_key(program_id, page_num);

    sp_io::storage::clear(&page_key);
}

pub fn insert_waiting_message(
    dest_prog_id: ProgramId,
    msg_id: MessageId,
    dispatch: StoredDispatch,
    bn: u32,
) {
    let payload = (dispatch, bn);
    sp_io::storage::set(&wait_key(dest_prog_id, msg_id), &payload.encode());
}

pub fn remove_waiting_message(
    dest_prog_id: ProgramId,
    msg_id: MessageId,
) -> Option<(StoredDispatch, u32)> {
    let id = wait_key(dest_prog_id, msg_id);
    let msg = sp_io::storage::get(&id)
        .and_then(|val| <(StoredDispatch, u32)>::decode(&mut &val[..]).ok());

    if msg.is_some() {
        sp_io::storage::clear(&id);
    }
    msg
}

pub fn waiting_init_prefix(prog_id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_STATE_WAIT_PREFIX);
    prog_id.encode_to(&mut key);

    key
}

fn program_waitlist_prefix(prog_id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    prog_id.encode_to(&mut key);

    key
}

pub fn remove_program_waitlist(prog_id: ProgramId) -> Vec<StoredDispatch> {
    let key = program_waitlist_prefix(prog_id);
    let messages =
        sp_io::storage::get(&key).and_then(|v| Vec::<StoredDispatch>::decode(&mut &v[..]).ok());
    sp_io::storage::clear(&key);

    messages.unwrap_or_default()
}

pub fn waiting_init_append_message_id(dest_prog_id: ProgramId, message_id: MessageId) {
    let key = waiting_init_prefix(dest_prog_id);
    sp_io::storage::append(&key, message_id.encode());
}

pub fn waiting_init_take_messages(dest_prog_id: ProgramId) -> Vec<MessageId> {
    let key = waiting_init_prefix(dest_prog_id);
    let messages =
        sp_io::storage::get(&key).and_then(|v| Vec::<MessageId>::decode(&mut &v[..]).ok());
    sp_io::storage::clear(&key);

    messages.unwrap_or_default()
}

pub fn code_exists(code_id: CodeId) -> bool {
    sp_io::storage::exists(&code_key(code_id, CodeKeyPrefixKind::RawCode))
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

    fn get_active_program(id: ProgramId) -> Option<ActiveProgram> {
        get_program(id).and_then(|p| p.try_into().ok())
    }

    #[test]
    fn program_decoded() {
        sp_io::TestExternalities::new_empty().execute_with(|| {
            let code = b"pretended wasm code".to_vec();
            let code_id = CodeId::generate(&code);
            let program_id: ProgramId = 1.into();
            let program = ActiveProgram {
                static_pages: 256,
                persistent_pages: Default::default(),
                code_id,
                state: ProgramState::Initialized,
            };
            set_code(code_id, &code);
            assert!(get_program(program_id).is_none());
            set_program(program_id, program.clone(), Default::default());
            assert_eq!(get_active_program(program_id).unwrap(), program);
            assert_eq!(get_code(program.code_id).unwrap(), code);
        });
    }
}
