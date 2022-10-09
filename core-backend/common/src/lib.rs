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
pub use utils::calc_stack_end;

#[cfg(feature = "mock")]
pub mod mock;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::{
    fmt::{self, Display},
    ops::Deref,
};
use gear_core::{
    buffer::RuntimeBufferSizeError,
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
    Wait(Option<u32>),
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
    #[display(fmt = "Reason is unknown. Possibly `unreachable` instruction is occurred")]
    Unknown,
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub allocations: Option<BTreeSet<WasmPageNumber>>,
    pub pages_data: BTreeMap<PageNumber, PageBuf>,
    pub generated_dispatches: Vec<(Dispatch, u32)>,
    pub awakening: Vec<(MessageId, u32)>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ProgramId)>>,
    pub context_store: ContextStore,
}

pub trait IntoExtInfo<Error> {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, (MemoryError, GasAmount)>;

    fn into_gas_amount(self) -> GasAmount;

    fn last_error(&self) -> Result<&ExtError, Error>;

    fn last_error_encoded(&self) -> Result<Vec<u8>, Error> {
        self.last_error().map(Encode::encode)
    }

    fn trap_explanation(&self) -> Option<TrapExplanation>;
}

pub trait GetGasAmount {
    fn gas_amount(&self) -> GasAmount;
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum RuntimeCtxError<E: Display> {
    #[display(fmt = "{}", _0)]
    Ext(E),
    #[from]
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{}", _0)]
    RuntimeBuffer(RuntimeBufferSizeError),
}

pub trait RuntimeCtx<E: Ext> {
    /// Allocate new pages in instance memory.
    fn alloc(&mut self, pages: u32) -> Result<WasmPageNumber, RuntimeCtxError<E::Error>>;

    /// Read designated chunk from the memory.
    fn read_memory(&self, ptr: i32, len: u32) -> Result<Vec<u8>, RuntimeCtxError<E::Error>>;

    /// Read designated chunk from the memory into the supplied buffer.
    fn read_memory_into_buf(
        &self,
        ptr: i32,
        buf: &mut [u8],
    ) -> Result<(), RuntimeCtxError<E::Error>>;

    /// Reads and decodes a type with a size fixed at compile time from program memory.
    fn read_memory_as<D: Decode + MaxEncodedLen>(
        &self,
        ptr: i32,
    ) -> Result<D, RuntimeCtxError<E::Error>>;

    /// Write the given buffer and its length to the designated locations in memory.
    //
    /// `out_ptr` is the location in memory where `buf` should be written to.
    fn write_output(&mut self, out_ptr: i32, buf: &[u8]) -> Result<(), RuntimeCtxError<E::Error>>;
}

pub struct BackendReport<T, E> {
    pub termination_reason: TerminationReason,
    pub memory_wrap: T,
    pub ext: E,
}

#[derive(Debug, derive_more::Display)]
pub enum StackEndError {
    #[display(fmt = "Stack end addr {:#x} cannot be negative", _0)]
    IsNegative(i32),
    #[display(fmt = "Stack end addr {:#x} must be aligned to WASM page size", _0)]
    IsNotAligned(i32),
}

// '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";

pub trait Environment<E: Ext + IntoExtInfo<E::Error> + 'static>: Sized {
    /// Memory type for current environment.
    type Memory: Memory;

    /// An error issues in environment.
    type Error: fmt::Display + GetGasAmount;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `pre_execution_handler` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    /// 5) Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<F, T>(
        ext: E,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, Self::Error>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPageNumber>) -> Result<(), T>,
        T: fmt::Display;
}

pub trait AsTerminationReason {
    fn as_termination_reason(&self) -> Option<&TerminationReason>;
}
