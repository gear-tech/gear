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

pub mod lazy_pages;

mod utils;
use memory::OutOfMemoryAccessError;
pub use utils::calc_stack_end;

#[cfg(feature = "mock")]
pub mod mock;

pub mod memory;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use codec::{Decode, Encode};
use core::{
    fmt::{self},
    mem::{self, MaybeUninit},
    ops::Deref,
    slice,
};
use gear_core::{
    env::Ext,
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, Memory, MemoryInterval, PageBuf, WasmPage},
    message::{
        ContextStore, Dispatch, DispatchKind, IncomingDispatch, MessageWaitedType, WasmEntry,
    },
    reservation::GasReserver,
};
use gear_core_errors::{ExtError, MemoryError};
use lazy_pages::GlobalsConfig;
use scale_info::TypeInfo;

pub trait IntoExtError: Sized {
    fn into_ext_error(self) -> Result<ExtError, Self>;

    fn into_termination_reason(self) -> TerminationReason;
}

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
    Wait(Option<u32>, MessageWaitedType),
    GasAllowanceExceeded,
}

#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
)]
pub enum TrapExplanation {
    #[display(fmt = "{_0}")]
    Core(ExtError),
    #[display(fmt = "{_0}")]
    Other(TrimmedString),
    #[display(fmt = "Reason is unknown. Possibly `unreachable` instruction is occurred")]
    Unknown,
}

#[derive(Debug, Default)]
pub struct SystemReservationContext {
    /// Reservation created in current execution.
    pub current_reservation: Option<u64>,
    /// Reservation from `ContextStore`.
    pub previous_reservation: Option<u64>,
}

impl SystemReservationContext {
    pub fn from_dispatch(dispatch: &IncomingDispatch) -> Self {
        Self {
            current_reservation: None,
            previous_reservation: dispatch
                .context()
                .as_ref()
                .and_then(|ctx| ctx.system_reservation()),
        }
    }

    pub fn has_any(&self) -> bool {
        self.current_reservation.is_some() || self.previous_reservation.is_some()
    }
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub gas_reserver: GasReserver,
    pub system_reservation_context: SystemReservationContext,
    pub allocations: BTreeSet<WasmPage>,
    pub pages_data: BTreeMap<GearPage, PageBuf>,
    pub generated_dispatches: Vec<(Dispatch, u32, Option<ReservationId>)>,
    pub awakening: Vec<(MessageId, u32)>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ProgramId)>>,
    pub context_store: ContextStore,
}

pub trait IntoExtInfo<Error> {
    fn into_ext_info(self, memory: &impl Memory) -> ExtInfo;

    fn into_gas_amount(self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
    ) -> Result<(), OutOfMemoryAccessError>;
}

pub trait GetGasAmount {
    fn gas_amount(&self) -> GasAmount;
}

/// Writes object in given memory as bytes.
pub fn write_memory_as<T: Sized>(
    memory: &mut impl Memory,
    ptr: u32,
    obj: T,
) -> Result<(), MemoryError> {
    // # Safety:
    //
    // Given object is `Sized` and we own them in the context of calling this
    // function (it's on stack), it's safe to take ptr on the object and
    // represent it as slice. Object will be dropped after `memory.write`
    // finished execution and no one will rely on this slice.
    //
    // Bytes in memory always stored continuously and without paddings, properly
    // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
    let slice =
        unsafe { slice::from_raw_parts(&obj as *const T as *const u8, mem::size_of::<T>()) };

    memory.write(ptr, slice)
}

/// Reads bytes from given pointer to construct type T from them.
pub fn read_memory_as<T: Sized>(memory: &impl Memory, ptr: u32) -> Result<T, MemoryError> {
    let mut buf = MaybeUninit::<T>::uninit();

    // # Safety:
    //
    // Usage of mutable slice is safe for the same reason from `write_memory_as`.
    // `MaybeUninit` is presented on stack with continuos sequence of bytes.
    //
    // It's also safe to construct T from any bytes, because we use the fn
    // only for reading primitive const-size types that are `[repr(C)]`,
    // so they always represented from sequence of bytes.
    //
    // Bytes in memory always stored continuously and without paddings, properly
    // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
    let mut_slice =
        unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, mem::size_of::<T>()) };

    memory.read(ptr, mut_slice)?;

    // # Safety:
    //
    // Assuming init is always safe here due to the fact that we read proper
    // amount of bytes from the wasm memory, which is never uninited: they may
    // be filled by zeroes or some trash (valid for our primitives used as T),
    // but always exist.
    Ok(unsafe { buf.assume_init() })
}

pub struct BackendReport<T, E> {
    pub termination_reason: TerminationReason,
    pub memory_wrap: T,
    pub ext: E,
}

#[derive(Debug, derive_more::Display)]
pub enum StackEndError {
    #[display(fmt = "Stack end addr {_0:#x} must be aligned to WASM page size")]
    IsNotAligned(u32),
}

// '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";

pub trait Environment<E, EP = DispatchKind>: Sized
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    EP: WasmEntry,
{
    /// Memory type for current environment.
    type Memory: Memory;

    /// An error issues in environment.
    type Error: fmt::Display + GetGasAmount;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `pre_execution_handler` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    fn new(
        ext: E,
        binary: &[u8],
        entry_point: EP,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, Self::Error>;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<F, T>(
        self,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, Self::Error>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPage>, GlobalsConfig) -> Result<(), T>,
        T: fmt::Display;
}
