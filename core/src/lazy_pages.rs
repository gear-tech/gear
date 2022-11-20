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

use core::fmt::Debug;

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

pub struct GlobalsAccessError;

pub trait GlobalsAccessTrait {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError>;
    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// pub struct GlobalAccessProvider<'a> {
//     pub inner_access_provider: &'a mut dyn GlobalsAccessTrait,
// }

// impl<'a> Debug for GlobalAccessProvider<'a> {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         f.debug_struct("GlobalAccessProvider").finish()
//     }
// }

// impl<'a> GlobalsAccessTrait for GlobalAccessProvider<'a> {
//     fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
//         self.inner_access_provider.get_i64(name)
//     }

//     fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
//         self.inner_access_provider.set_i64(name, value)
//     }

//     fn as_any_mut(&'static mut self) -> &mut dyn Any {
//         self
//     }
// }
