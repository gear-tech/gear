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

//! Globals access for lazy-pages.

use core::{fmt::Debug, convert::TryFrom};

use crate::memory::HostPointer;
use alloc::string::String;
use codec::{Decode, Encode};
use core::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum GlobalsAccessMod {
    WasmRuntime,
    NativeRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LazyPagesCosts {
    pub read_page: u64,
    pub write_page: u64,
    pub update_page: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GlobalsCtx {
    pub global_gas_name: String,
    pub global_allowance_name: String,
    pub global_state_name: String,
    pub lazy_pages_costs: LazyPagesCosts,
    pub globals_access_ptr: HostPointer,
    pub globals_access_mod: GlobalsAccessMod,
}

#[derive(Debug)]
pub struct GlobalsAccessError;

pub trait GlobalsAccessTrait {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError>;
    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// pub type StatusNo = u64;
// pub const STATUS_NORMAL: StatusNo = 0;
// pub const STATUS_GAS_LIMIT_EXCEEDED: StatusNo = 1;
// pub const STATUS_GAS_ALLOWANCE_EXCEEDED: StatusNo = 2;
// pub const STATUS_MEMORY_ACCESS_OUT_OF_BOUNDS: StatusNo = 3;

#[derive(Debug, Clone, Copy, Encode, Decode)]
#[repr(i64)]
pub enum Status {
    Normal = 0,
    GasLimitExceeded,
    GasAllowanceExceeded,
    MemoryAccessOutOfBounds,
}

// #[derive(Debug)]
// pub struct StatusWrongNo(StatusNo);

// impl TryFrom<StatusNo> for Status {
//     type Error = StatusWrongNo;
//     fn try_from(value: StatusNo) -> Result<Status, StatusWrongNo> {
//         match value {
//             STATUS_NORMAL => Ok(Self::Normal),
//             STATUS_GAS_LIMIT_EXCEEDED => Ok(Self::GasLimitExceeded),
//             STATUS_GAS_ALLOWANCE_EXCEEDED => Ok(Self::GasAllowanceExceeded),
//             STATUS_MEMORY_ACCESS_OUT_OF_BOUNDS => Ok(Self::MemoryAccessOutOfBounds),
//             wrong => Err(StatusWrongNo(wrong)),
//         }
//     }
// }
