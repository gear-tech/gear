// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Trait that both sandbox and wasmi runtimes must implement.

use crate::{
    memory::{MemoryAccessError, MemoryAccessRecorder, MemoryOwner, WasmMemoryWrite},
    BackendExternalities, BackendState, TerminationReason,
};
use gear_core::{costs::RuntimeCosts, gas::GasLeft, memory::WasmPage};
use gear_core_errors::ExtError;

pub trait Runtime<Ext: BackendExternalities>:
    MemoryOwner + MemoryAccessRecorder + BackendState
{
    type Error;

    fn unreachable_error() -> Self::Error;

    fn fallible_syscall_error(&self) -> Option<&ExtError>;

    fn ext_mut(&mut self) -> &mut Ext;

    fn run_any<T, F>(&mut self, cost: RuntimeCosts, f: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, TerminationReason>;

    fn run_fallible<T: Sized, F, R>(
        &mut self,
        res_ptr: u32,
        cost: RuntimeCosts,
        f: F,
    ) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, TerminationReason>,
        R: From<Result<T, u32>> + Sized;

    fn alloc(&mut self, pages: u32) -> Result<WasmPage, Ext::AllocError>;

    fn memory_manager_write(
        &mut self,
        write: WasmMemoryWrite,
        buff: &[u8],
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError>;
}
