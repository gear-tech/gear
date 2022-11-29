// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use core::fmt::Debug;

use crate::memory::HostPointer;
use alloc::string::String;
use codec::{Decode, Encode};
use core::any::Any;

/// Informs lazy-pages whether they work with native or WASM runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum GlobalsAccessMod {
    /// Is wasm runtime.
    WasmRuntime,
    /// Is native runtime.
    NativeRuntime,
}

/// Lazy-pages cases weights.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LazyPagesWeights {
    /// Read one gear page weight.
    pub read: u64,
    /// Write to one gear page weight.
    pub write: u64,
    /// Write to one gear page weight, which has been already read accessed.
    pub write_after_read: u64,
}

/// Globals ctx for lazy-pages initialization.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GlobalsCtx {
    /// Gas amount global name.
    pub global_gas_name: String,
    /// Gas allowance amount global name.
    pub global_allowance_name: String,
    /// Gear status global name.
    pub global_state_name: String,
    /// Lazy-pages access weights.
    pub lazy_pages_weights: LazyPagesWeights,
    /// Raw pointer to the globals access provider.
    pub globals_access_ptr: HostPointer,
    /// Access mod, currently two: native or WASM runtime.
    pub globals_access_mod: GlobalsAccessMod,
}

/// Globals access error.
#[derive(Debug)]
pub struct GlobalsAccessError;

/// Globals access trait.
pub trait GlobalsAccessTrait {
    /// Returns global `name` value, if `name` is I64 global export.
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError>;
    /// Set global `name` == `value`, if `name` is I64 global export.
    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError>;
    /// Returns global `name` value, if `name` is I32 global export.
    fn get_i32(&self, name: &str) -> Result<i32, GlobalsAccessError>;
    /// Set global `name` == `value`, if `name` is I32 global export.
    fn set_i32(&mut self, name: &str, value: i32) -> Result<(), GlobalsAccessError>;
    /// Returns as `&mut syn Any`.
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
#[derive(Debug, Clone, Copy, Encode, Decode)]
#[repr(i64)]
pub enum Status {
    /// Lazy-pages works in normal mode.
    Normal = 0_i64,
    /// Skips signals processing until the end of execution, set termination reason as `gas limit exceeded`.
    GasLimitExceeded,
    /// Skips signals processing until the end of execution, set termination reason as `gas allowance exceeded`.
    GasAllowanceExceeded,
}

/// Memory access error.
#[derive(Debug, Clone, Copy, Decode, Encode, derive_more::Display)]
pub enum AccessError {
    /// Given access addr + size overflows u32.
    #[display(fmt = "Access interval addr {:#x} + size {:#x} overflows u32::MAX", _0, _1)]
    AddrPlusSizeOverflow(u32, u32),
    /// Access size cannot be less then 1 byte.
    #[display(fmt = "Access interval size is zero")]
    AccessSizeIsZero,
    /// Access is out of wasm memory.
    #[display(fmt = "Access is out of wasm wasm memory")]
    OutOfWasmMemoryAccess,
}
