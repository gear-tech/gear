// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

//! Core logic for usage both in runtime and in lazy-pages native part.

#![no_std]

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::{any::Any, fmt::Debug};
use gear_core::{
    costs::LazyPagesCosts,
    ids::ActorId,
    memory::{HostPointer, Memory, MemoryInterval},
    pages::{GearPage, WasmPage, WasmPagesAmount},
    program::MemoryInfix,
    str::LimitedStr,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use parity_scale_codec::{Decode, Encode};

// TODO #3057
const GLOBAL_NAME_GAS: &str = "gear_gas";

/// Memory access error during syscall that lazy-pages have caught.
/// 0 index is reserved for an ok result.
#[derive(Debug, Copy, Clone, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum ProcessAccessError {
    OutOfBounds = 1,
    GasLimitExceeded = 2,
}

/// Informs lazy-pages whether they work with native or WASM runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum GlobalsAccessMod {
    /// Is wasm runtime.
    WasmRuntime,
    /// Is native runtime.
    NativeRuntime,
}

/// Globals ctx for lazy-pages initialization for program.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GlobalsAccessConfig {
    /// Raw pointer to the globals access provider.
    pub access_ptr: HostPointer,
    /// Access mod, currently two: native or WASM runtime.
    pub access_mod: GlobalsAccessMod,
}

/// Globals access error.
#[derive(Debug)]
pub struct GlobalsAccessError;

/// Globals access trait.
pub trait GlobalsAccessor {
    /// Returns global `name` value, if `name` is I64 global export.
    fn get_i64(&mut self, name: &LimitedStr) -> Result<i64, GlobalsAccessError>;

    /// Set global `name` == `value`, if `name` is I64 global export.
    fn set_i64(&mut self, name: &LimitedStr, value: i64) -> Result<(), GlobalsAccessError>;

    /// Returns global `name` value, if `name` is I32 global export.
    fn get_i32(&self, _name: &LimitedStr) -> Result<i32, GlobalsAccessError> {
        unimplemented!("Currently has no i32 system globals")
    }

    /// Set global `name` == `value`, if `name` is I32 global export.
    fn set_i32(&mut self, _name: &LimitedStr, _value: i32) -> Result<(), GlobalsAccessError> {
        unimplemented!("Currently has no i32 system globals")
    }

    /// Returns as `&mut dyn Any`.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Lazy-pages status.
/// By default in program initialization status is set as `Normal`.
/// If nothing bad happens in lazy-pages, then status remains to be `Normal`.
/// If gas limit exceed, then status is set as `GasLimitExceeded`, and lazy-pages
/// starts to skips all signals processing until the end of execution.
/// The same is for gas allowance exceed, except it sets status as `GasAllowanceExceed`.
/// In the end of execution this status is checked and if it's not `Normal` then
/// termination reason sets as `gas limit exceeded` or `gas allowance exceeded`, depending on status.
/// NOTE: `repr(i64)` is important to be able add additional fields, without old runtimes separate support logic.
#[derive(Debug, Clone, Copy, Encode, Decode, PartialEq, Eq)]
#[repr(i64)]
// TODO: consider removal of two exceed options in favor of one global (issue #3018).
// Will require bump of many RI func's versions.
pub enum Status {
    /// Lazy-pages works in normal mode.
    Normal = 0_i64,
    /// Skips signals processing until the end of execution, set termination reason as `gas limit exceeded`.
    GasLimitExceeded,
}

impl Status {
    /// Returns bool defining if status is `Normal`.
    pub fn is_normal(&self) -> bool {
        *self == Self::Normal
    }
}

#[derive(Debug, Clone)]
pub struct LazyPagesInitContext {
    pub page_sizes: Vec<u32>,
    pub global_names: Vec<LimitedStr<'static>>,
    pub pages_storage_prefix: Vec<u8>,
}

impl LazyPagesInitContext {
    pub fn new(prefix: [u8; 32]) -> Self {
        Self {
            page_sizes: vec![WasmPage::SIZE, GearPage::SIZE],
            global_names: vec![LimitedStr::from_small_str(GLOBAL_NAME_GAS)],
            pages_storage_prefix: prefix.to_vec(),
        }
    }
}

pub trait LazyPagesInterface {
    /// Try to enable and initialize lazy pages env
    fn try_to_enable_lazy_pages(prefix: [u8; 32]) -> bool;

    /// Protect and save storage keys for pages which has no data
    fn init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        program_id: ActorId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        costs: LazyPagesCosts,
    );

    /// Remove lazy-pages protection, returns wasm memory begin addr
    fn remove_lazy_pages_prot<Context>(ctx: &mut Context, mem: &mut impl Memory<Context>);

    /// Protect lazy-pages and set new wasm mem addr and size,
    /// if they have been changed.
    fn update_lazy_pages_and_protect_again<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        old_mem_addr: Option<HostPointer>,
        old_mem_size: WasmPagesAmount,
        new_mem_addr: HostPointer,
    );

    /// Returns list of released pages numbers.
    fn get_write_accessed_pages() -> Vec<GearPage>;

    /// Returns lazy pages actual status.
    fn get_status() -> Status;

    /// Pre-process memory access in syscalls in lazy-pages.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError>;
}

impl LazyPagesInterface for () {
    fn try_to_enable_lazy_pages(_prefix: [u8; 32]) -> bool {
        unimplemented!()
    }

    fn init_for_program<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _program_id: ActorId,
        _memory_infix: MemoryInfix,
        _stack_end: Option<WasmPage>,
        _globals_config: GlobalsAccessConfig,
        _costs: LazyPagesCosts,
    ) {
        unimplemented!()
    }

    fn remove_lazy_pages_prot<Context>(_ctx: &mut Context, _mem: &mut impl Memory<Context>) {
        unimplemented!()
    }

    fn update_lazy_pages_and_protect_again<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _old_mem_addr: Option<HostPointer>,
        _old_mem_size: WasmPagesAmount,
        _new_mem_addr: HostPointer,
    ) {
        unimplemented!()
    }

    fn get_write_accessed_pages() -> Vec<GearPage> {
        unimplemented!()
    }

    fn get_status() -> Status {
        unimplemented!()
    }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        unimplemented!()
    }
}
