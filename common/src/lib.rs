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

#[macro_use]
extern crate gear_common_codegen;

pub mod event;
pub mod lazy_pages;
pub mod scheduler;
pub mod storage;

pub mod code_storage;
pub use code_storage::{CodeStorage, Error as CodeStorageError};

pub mod gas_provider;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use codec::{Decode, Encode};
use core::{fmt, mem};
use frame_support::{
    dispatch::DispatchError,
    traits::Get,
    weights::{IdentityFee, WeightToFee},
};
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{Error as MemoryError, PageBuf, PageNumber, WasmPageNumber},
};
use primitive_types::H256;
use scale_info::TypeInfo;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_core::crypto::UncheckedFrom;
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};
use storage::ValueStorage;

pub use gas_provider::{Provider as GasProvider, Tree as GasTree};

pub const STORAGE_PROGRAM_PREFIX: &[u8] = b"g::prog::";
pub const STORAGE_PROGRAM_PAGES_PREFIX: &[u8] = b"g::pages::";
pub const STORAGE_PROGRAM_STATE_WAIT_PREFIX: &[u8] = b"g::prog_wait::";

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
        IdentityFee::<Self::Balance>::weight_to_fee(&gas)
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

/// Contains various limits for the block.
pub trait BlockLimiter {
    /// The maximum amount of gas that can be used within a single block.
    type BlockGasLimit: Get<Self::Balance>;

    /// Type representing a quantity of value.
    type Balance;

    /// Type manages a gas that is available at the moment of call.
    type GasAllowance: storage::Limiter<Value = Self::Balance>;
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
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

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct ActiveProgram {
    /// Set of wasm pages numbers, which are allocated by the program.
    pub allocations: BTreeSet<WasmPageNumber>,
    /// Set of gear pages numbers, which has data in storage.
    pub pages_with_data: BTreeSet<PageNumber>,
    pub code_hash: H256,
    pub state: ProgramState,
}

/// Enumeration contains variants for program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub enum ProgramState {
    /// `init` method of a program has not yet finished its execution so
    /// the program is not considered as initialized. All messages to such a
    /// program go to the wait list.
    /// `message_id` contains identifier of the initialization message.
    Uninitialized { message_id: MessageId },
    /// Program has been successfully initialized and can process messages.
    Initialized,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
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
    let id_bytes = program_id.as_fixed_bytes();
    let mut key = Vec::with_capacity(STORAGE_PROGRAM_PAGES_PREFIX.len() + id_bytes.len() + 2);
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    key.extend(program_id.as_fixed_bytes());
    key.extend(b"::");

    key
}

fn page_key(id: H256, page: PageNumber) -> Vec<u8> {
    // try to avoid realloc
    let id_bytes = id.as_fixed_bytes();
    let mut key = Vec::with_capacity(
        STORAGE_PROGRAM_PAGES_PREFIX.len() + id_bytes.len() + 2 + mem::size_of::<u32>(),
    );
    key.extend(STORAGE_PROGRAM_PAGES_PREFIX);
    key.extend(id.as_fixed_bytes());
    key.extend(b"::");
    key.extend(page.0.to_le_bytes());

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
pub fn get_program_page_data(
    id: H256,
    page_idx: PageNumber,
) -> Option<Result<PageBuf, MemoryError>> {
    let key = page_key(id, page_idx);
    let data = sp_io::storage::get(&key)?;
    Some(PageBuf::new_from_vec(data))
}

pub fn get_program_pages_data(
    id: H256,
    program: &ActiveProgram,
) -> Result<BTreeMap<PageNumber, PageBuf>, MemoryError> {
    get_program_data_for_pages(id, program.pages_with_data.iter())
}

/// Returns data for all pages from `pages` arg, which has data in storage.
pub fn get_program_data_for_pages<'a>(
    id: H256,
    pages: impl Iterator<Item = &'a PageNumber>,
) -> Result<BTreeMap<PageNumber, PageBuf>, MemoryError> {
    let mut pages_data = BTreeMap::new();
    for page in pages {
        let key = page_key(id, *page);
        let data = sp_io::storage::get(&key);
        if let Some(data) = data {
            let page_buf = PageBuf::new_from_vec(data)?;
            pages_data.insert(*page, page_buf);
        }
    }
    Ok(pages_data)
}

pub fn set_program(id: H256, program: ActiveProgram) {
    log::trace!("set program with id = {}", id);
    sp_io::storage::set(&program_key(id), &Program::Active(program).encode());
}

#[derive(Debug)]
pub struct PageIsNotAllocatedErr(pub PageNumber);

impl fmt::Display for PageIsNotAllocatedErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Page #{:?} is not allocated for current program",
            self.0 .0
        )
    }
}

pub fn set_program_and_pages_data(
    id: H256,
    program: ActiveProgram,
    persistent_pages: BTreeMap<PageNumber, PageBuf>,
) -> Result<(), PageIsNotAllocatedErr> {
    for (page_num, page_buf) in persistent_pages {
        if !program.allocations.contains(&page_num.to_wasm_page()) {
            return Err(PageIsNotAllocatedErr(page_num));
        }
        let key = page_key(id, page_num);
        sp_io::storage::set(&key, page_buf.as_slice());
    }
    set_program(id, program);
    Ok(())
}

pub fn program_exists(id: H256) -> bool {
    sp_io::storage::exists(&program_key(id))
}

pub fn set_program_allocations(id: H256, allocations: BTreeSet<WasmPageNumber>) {
    if let Some(Program::Active(mut prog)) = get_program(id) {
        prog.allocations = allocations;
        sp_io::storage::set(&program_key(id), &Program::Active(prog).encode())
    }
}

pub fn set_program_page_data(program_id: H256, page: PageNumber, page_buf: PageBuf) {
    let page_key = page_key(program_id, page);
    sp_io::storage::set(&page_key, page_buf.as_slice());
}

pub fn remove_program_page_data(program_id: H256, page_num: PageNumber) {
    let page_key = page_key(program_id, page_num);
    sp_io::storage::clear(&page_key);
}

pub fn waiting_init_prefix(prog_id: ProgramId) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PROGRAM_STATE_WAIT_PREFIX);
    prog_id.encode_to(&mut key);

    key
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

pub fn reset_storage() {
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PREFIX, None);
    sp_io::storage::clear_prefix(STORAGE_PROGRAM_PAGES_PREFIX, None);

    // TODO: Remove this legacy after next runtime upgrade.
    sp_io::storage::clear_prefix(b"g::wait::", None);
}
