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

//! Crate provides support for wasm runtime.

#![no_std]

extern crate alloc;

pub mod error_processor;

mod utils;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::{fmt, ops::Deref};
use error_processor::IntoExtError;
use gear_core::{
    env::Ext,
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId},
    memory::{Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextStore, Dispatch, DispatchKind},
};
use gear_core_errors::{ExtError, MemoryError};
use scale_info::TypeInfo;

// Max amount of bytes allowed to be thrown as string explanation of the error.
pub const TRIMMED_MAX_LEN: usize = 1024;

/// Wrapped string to fit `core-backend::TRIMMED_MAX_LEN` amount of bytes.
#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
)]
pub struct TrimmedString(String);

impl TrimmedString {
    pub(crate) fn new(mut string: String) -> Self {
        utils::smart_truncate(&mut string, TRIMMED_MAX_LEN);
        Self(string)
    }
}

impl<T: Into<String>> From<T> for TrimmedString {
    fn from(other: T) -> Self {
        Self::new(other.into())
    }
}

impl Deref for TrimmedString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum TerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Trap(TrapExplanation),
    Wait,
    GasAllowanceExceeded,
}

#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
)]
pub enum TrapExplanation {
    #[display(fmt = "{}", _0)]
    Core(ExtError),
    #[display(fmt = "{}", _0)]
    Other(TrimmedString),
    #[display(fmt = "Unable to call a forbidden function")]
    ForbiddenFunction,
    #[display(fmt = "Reason is unknown. Possibly `unreachable` instruction is occurred")]
    Unknown,
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub allocations: Option<BTreeSet<WasmPageNumber>>,
    pub pages_data: BTreeMap<PageNumber, PageBuf>,
    pub generated_dispatches: Vec<Dispatch>,
    pub awakening: Vec<MessageId>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    pub context_store: ContextStore,
}

pub trait IntoExtInfo {
    fn into_ext_info(
        self,
        memory: &impl Memory,
        stack_page_count: WasmPageNumber,
    ) -> Result<ExtInfo, (MemoryError, GasAmount)>;

    fn into_gas_amount(self) -> GasAmount;

    fn last_error(&self) -> Option<&ExtError>;

    fn trap_explanation(&self) -> Option<TrapExplanation>;
}

pub trait RuntimeCtx<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    /// Allocate new pages in instance memory.
    fn alloc(&mut self, pages: u32) -> Result<gear_core::memory::WasmPageNumber, E::Error>;

    /// Read designated chunk from the memory.
    fn read_memory(&self, ptr: u32, len: u32) -> Result<Vec<u8>, MemoryError>;

    /// Read designated chunk from the memory into the supplied buffer.
    fn read_memory_into_buf(&self, ptr: u32, buf: &mut [u8]) -> Result<(), MemoryError>;

    /// Reads and decodes a type with a size fixed at compile time from program memory.
    fn read_memory_as<D: Decode + MaxEncodedLen>(&self, ptr: u32) -> Result<D, MemoryError>;

    /// Write the given buffer and its length to the designated locations in memory.
    //
    /// `out_ptr` is the location in memory where `buf` should be written to.
    fn write_output(&mut self, out_ptr: u32, buf: &[u8]) -> Result<(), MemoryError>;
}

pub struct BackendReport<T> {
    pub termination_reason: TerminationReason,
    pub memory_wrap: T,
    pub stack_end_page: Option<WasmPageNumber>,
}

#[derive(Debug, derive_more::Display)]
#[display(fmt = "{}", reason)]
pub struct BackendError<T> {
    pub reason: T,
}

pub trait Environment<E: Ext + IntoExtInfo + 'static>: Sized {
    /// Memory type for current environment.
    type Memory: Memory;

    /// An error issues in environment.
    type Error: fmt::Display;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `pre_execution_handler` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    /// 5) Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<F, T>(
        ext: &mut E,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory>, BackendError<Self::Error>>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPageNumber>) -> Result<(), T>,
        T: fmt::Display;
}

pub trait AsTerminationReason {
    fn as_termination_reason(&self) -> Option<&TerminationReason>;
}
