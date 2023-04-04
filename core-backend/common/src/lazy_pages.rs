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

use core::any::Any;
use gear_core::{
    costs::CostPerPage,
    memory::{GearPage, HostPointer},
};
use scale_info::scale::{self, Decode, Encode};

/// Informs lazy-pages whether they work with native or WASM runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[codec(crate = scale)]
pub enum GlobalsAccessMod {
    /// Is wasm runtime.
    WasmRuntime,
    /// Is native runtime.
    NativeRuntime,
}

/// Lazy-pages cases weights.
#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[codec(crate = scale)]
pub struct LazyPagesWeights {
    /// First read page access cost.
    pub signal_read: CostPerPage<GearPage>,
    /// First write page access cost.
    pub signal_write: CostPerPage<GearPage>,
    /// First write access cost for page, which has been already read accessed.
    pub signal_write_after_read: CostPerPage<GearPage>,
    /// First read page access cost from host function call.
    pub host_func_read: CostPerPage<GearPage>,
    /// First write page access cost from host function call.
    pub host_func_write: CostPerPage<GearPage>,
    /// First write page access cost from host function call.
    pub host_func_write_after_read: CostPerPage<GearPage>,
    /// Loading page data from storage cost.
    pub load_page_storage_data: CostPerPage<GearPage>,
}

/// Globals ctx for lazy-pages initialization for program.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[codec(crate = scale)]
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
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError>;
    /// Set global `name` == `value`, if `name` is I64 global export.
    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError>;
    /// Returns global `name` value, if `name` is I32 global export.
    fn get_i32(&self, _name: &str) -> Result<i32, GlobalsAccessError> {
        unimplemented!("Currently has no i32 system globals")
    }
    /// Set global `name` == `value`, if `name` is I32 global export.
    fn set_i32(&mut self, _name: &str, _value: i32) -> Result<(), GlobalsAccessError> {
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
#[codec(crate = scale)]
#[repr(i64)]
pub enum Status {
    /// Lazy-pages works in normal mode.
    Normal = 0_i64,
    /// Skips signals processing until the end of execution, set termination reason as `gas limit exceeded`.
    GasLimitExceeded,
    /// Skips signals processing until the end of execution, set termination reason as `gas allowance exceeded`.
    GasAllowanceExceeded,
}
