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

pub mod code_storage;
pub use code_storage::{CodeStorage, Error as CodeStorageError};

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use codec::{Decode, Encode};
use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    traits::Imbalance,
    weights::{IdentityFee, WeightToFeePolynomial},
};
use gear_runtime_interface as gear_ri;
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::crypto::UncheckedFrom;
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageNumber, WasmPageNumber},
    message::StoredDispatch,
};

pub use storage_queue::Iterator as QueueIter;
pub use storage_queue::StorageQueue;

pub const STORAGE_PROGRAM_PREFIX: &[u8] = b"g::prog::";
pub const STORAGE_PROGRAM_PAGES_PREFIX: &[u8] = b"g::pages::";
pub const STORAGE_PROGRAM_STATE_WAIT_PREFIX: &[u8] = b"g::prog_wait::";
pub const STORAGE_MESSAGE_PREFIX: &[u8] = b"g::msg::";
pub const STORAGE_MESSAGE_USER_NONCE_KEY: &[u8] = b"g::msg::user_nonce";
pub const STORAGE_CODE_METADATA_PREFIX: &[u8] = b"g::code::metadata::";
pub const STORAGE_WAITLIST_PREFIX: &[u8] = b"g::wait::";

pub const GAS_VALUE_PREFIX: &[u8] = b"g::gas_tree";

pub type ExitCode = i32;

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

impl Origin for MessageId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

impl Origin for ProgramId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
    }
}

impl Origin for CodeId {
    fn into_origin(self) -> H256 {
        H256(self.into())
    }

    fn from_origin(val: H256) -> Self {
        val.to_fixed_bytes().into()
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
        source: H256,
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

    /// Get the external origin for a key, if the latter exists.
    fn get_origin(key: Self::Key) -> Option<Self::ExternalOrigin>;

    /// Get value item by it's ID, if exists, and the key of an ancestor that sets this limit.
    fn get_limit(key: Self::Key) -> Option<(Self::Balance, Self::Key)>;

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

impl core::convert::TryFrom<Program> for ActiveProgram {
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
    /// Set of wasm pages numbers, which is allocated by the program.
    pub allocations: BTreeSet<WasmPageNumber>,
    /// Set of gear pages numbers, which has data in storage.
    pub pages_with_data: BTreeSet<PageNumber>,
    pub code_hash: H256,
    pub state: ProgramState,
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

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
pub struct CodeMetadata {
    pub author: H256,
    #[codec(compact)]
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

pub fn program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn pages_prefix(program_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    program_id.encode_to(&mut key);

    key
}

fn page_key(id: H256, page: PageNumber) -> Vec<u8> {
    let mut key = pages_prefix(id);
    key.extend(b"::");
    page.0.encode_to(&mut key);
    key
}

pub fn wait_prefix(prog_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    prog_id.encode_to(&mut key);
    key.extend(b"::");
    key
}

pub fn wait_key(prog_id: H256, msg_id: H256) -> Vec<u8> {
    let mut key = wait_prefix(prog_id);
    msg_id.encode_to(&mut key);
    key
}

pub fn set_program_initialized(id: H256) {
    if let Some(Program::Active(mut p)) = get_program(id) {
        if !matches!(p.state, ProgramState::Initialized) {
            p.state = ProgramState::Initialized;
            sp_io::storage::set(&program_key(id), &Program::Active(p).encode());
        }
    }
}

pub fn set_program_terminated_status(id: H256) -> Result<(), ProgramError> {
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

pub fn get_program(id: H256) -> Option<Program> {
    sp_io::storage::get(&program_key(id))
        .map(|val| Program::decode(&mut &val[..]).expect("values encoded correctly"))
}

/// Returns mem page data from storage for program `id` and `page_idx`
pub fn get_program_page_data(id: H256, page_idx: PageNumber) -> Option<Vec<u8>> {
    let key = page_key(id, page_idx);
    sp_io::storage::get(&key)
}

/// Save page data key in storage
pub fn save_page_lazy_info(id: H256, page_num: PageNumber) {
    let key = page_key(id, page_num);
    gear_ri::gear_ri::save_page_lazy_info(page_num.0, &key);
}

pub fn get_program_pages_data(id: H256, program: &ActiveProgram) -> BTreeMap<PageNumber, Vec<u8>> {
    get_program_data_for_pages(id, program.pages_with_data.iter())
}

/// Returns data for all pages from `pages` arg, which has data in storage.
pub fn get_program_data_for_pages<'a>(
    id: H256,
    pages: impl Iterator<Item = &'a PageNumber>,
) -> BTreeMap<PageNumber, Vec<u8>> {
    pages
        .map(|p| {
            let key = page_key(id, *p);
            (*p, sp_io::storage::get(&key))
        })
        .filter_map(|(page, data)| data.map(|data| (page, data)))
        .collect()
}

pub fn set_program(id: H256, program: ActiveProgram) {
    log::debug!("set program with id = {}", id);
    sp_io::storage::set(&program_key(id), &Program::Active(program).encode());
}

pub fn set_program_and_pages_data(
    id: H256,
    program: ActiveProgram,
    persistent_pages: BTreeMap<PageNumber, Vec<u8>>,
) {
    for (page_num, page_buf) in persistent_pages {
        // TODO: remove this panic and make result (issue 883)
        assert!(program.allocations.contains(&page_num.to_wasm_page()));
        let key = page_key(id, page_num);
        sp_io::storage::set(&key, &page_buf);
    }
    set_program(id, program);
}

pub fn program_exists(id: H256) -> bool {
    sp_io::storage::exists(&program_key(id))
}

pub fn clear_dispatch_queue() {
    sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
}

pub fn dequeue_dispatch() -> Option<StoredDispatch> {
    let mut dispatch_queue = StorageQueue::get(STORAGE_MESSAGE_PREFIX);
    dispatch_queue.dequeue()
}

pub fn queue_dispatch(dispatch: StoredDispatch) {
    let mut dispatch_queue = StorageQueue::get(STORAGE_MESSAGE_PREFIX);
    let id = dispatch.id();
    dispatch_queue.queue(dispatch, id.into_origin());
}

pub fn queue_dispatch_first(dispatch: StoredDispatch) {
    let mut dispatch_queue = StorageQueue::get(STORAGE_MESSAGE_PREFIX);
    let id = dispatch.id();
    dispatch_queue.queue_first(dispatch, id.into_origin());
}

pub fn dispatch_iter() -> QueueIter<StoredDispatch> {
    StorageQueue::get(STORAGE_MESSAGE_PREFIX).into_iter()
}

pub fn set_program_allocations(id: H256, allocations: BTreeSet<WasmPageNumber>) {
    if let Some(Program::Active(mut prog)) = get_program(id) {
        prog.allocations = allocations;
        sp_io::storage::set(&program_key(id), &Program::Active(prog).encode())
    }
}

pub fn set_program_page_data(program_id: H256, page: PageNumber, page_buf: Vec<u8>) {
    let page_key = page_key(program_id, page);
    sp_io::storage::set(&page_key, &page_buf);
}

pub fn remove_program_page_data(program_id: H256, page_num: PageNumber) {
    let page_key = page_key(program_id, page_num);
    sp_io::storage::clear(&page_key);
}

pub fn insert_waiting_message(dest_prog_id: H256, msg_id: H256, dispatch: StoredDispatch, bn: u32) {
    let payload = (dispatch, bn);
    sp_io::storage::set(&wait_key(dest_prog_id, msg_id), &payload.encode());
}

pub fn remove_waiting_message(dest_prog_id: H256, msg_id: H256) -> Option<(StoredDispatch, u32)> {
    let id = wait_key(dest_prog_id, msg_id);
    let msg = sp_io::storage::get(&id)
        .and_then(|val| <(StoredDispatch, u32)>::decode(&mut &val[..]).ok());

    if msg.is_some() {
        sp_io::storage::clear(&id);
    }
    msg
}

pub fn waiting_init_prefix(prog_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_STATE_WAIT_PREFIX);
    prog_id.encode_to(&mut key);

    key
}

fn program_waitlist_prefix(prog_id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_WAITLIST_PREFIX);
    prog_id.encode_to(&mut key);

    key
}

pub fn remove_program_waitlist(prog_id: H256) -> Vec<StoredDispatch> {
    let key = program_waitlist_prefix(prog_id);
    let messages =
        sp_io::storage::get(&key).and_then(|v| Vec::<StoredDispatch>::decode(&mut &v[..]).ok());
    sp_io::storage::clear(&key);

    messages.unwrap_or_default()
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

pub fn reset_storage() {
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PAGES_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_WAITLIST_PREFIX, None);
    sp_io::storage::clear_prefix(GAS_VALUE_PREFIX, None);

    // TODO (871) remove next lines after runtime upgraded
    pub const STORAGE_CODE_PREFIX: &[u8] = b"g::code::";
    pub const STORAGE_ORIGINAL_CODE_PREFIX: &[u8] = b"g::code::orig";
    sp_io::storage::clear_prefix(STORAGE_CODE_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_ORIGINAL_CODE_PREFIX, None);
}
