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

//! Lazy-pages system signals accesses support.

use crate::{
    common::{Error, GasLeftCharger, LazyPagesExecutionContext, WeightNo},
    globals::{self, GlobalNo},
    pages::{GearPageNumber, PageDynSize},
    process::{self, AccessHandler},
    LAZY_PAGES_CONTEXT,
};
use gear_backend_common::lazy_pages::Status;
use gear_core::gas::GasLeft;
use std::convert::TryFrom;

pub(crate) trait UserSignalHandler {
    /// # Safety
    ///
    /// It's expected handler calls sys-calls to protect memory
    unsafe fn handle(info: ExceptionInfo) -> Result<(), Error>;
}

pub(crate) struct DefaultUserSignalHandler;

impl UserSignalHandler for DefaultUserSignalHandler {
    unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
        user_signal_handler(info)
    }
}

#[derive(Debug)]
pub(crate) struct ExceptionInfo {
    /// Address where fault is occurred
    pub fault_addr: *const (),
    pub is_write: Option<bool>,
}

/// Before contract execution some pages from wasm memory buffer have been protected.
/// When wasm executer tries to access one of these pages,
/// OS emits sigsegv or sigbus or EXCEPTION_ACCESS_VIOLATION.
/// Using OS signal info, this function identifies memory page, which is accessed,
/// and process access for this page. See more in `process::process_lazy_pages`.
/// After processing is done, OS returns execution to the same machine
/// instruction, which cause signal. Now memory which this instruction accesses
/// is not protected and with correct data.
unsafe fn user_signal_handler_internal(
    ctx: &mut LazyPagesExecutionContext,
    info: ExceptionInfo,
) -> Result<(), Error> {
    let native_addr = info.fault_addr as usize;
    let is_write = info.is_write.ok_or_else(|| Error::ReadOrWriteIsUnknown)?;
    let wasm_mem_addr = ctx
        .wasm_mem_addr
        .ok_or_else(|| Error::WasmMemAddrIsNotSet)?;

    if native_addr < wasm_mem_addr {
        return Err(Error::OutOfWasmMemoryAccess);
    }

    let offset =
        u32::try_from(native_addr - wasm_mem_addr).map_err(|_| Error::OutOfWasmMemoryAccess)?;
    let page = GearPageNumber::from_offset(ctx, offset);

    let gas_ctx = if let Some(globals_config) = ctx.globals_context.as_ref() {
        let gas = globals::apply_for_global(globals_config, GlobalNo::GasLimit, |_| Ok(None))?;
        let allowance =
            globals::apply_for_global(globals_config, GlobalNo::AllowanceLimit, |_| Ok(None))?;
        let gas_left_charger = GasLeftCharger {
            read_cost: ctx.weight(WeightNo::SignalRead),
            write_cost: ctx.weight(WeightNo::SignalWrite),
            write_after_read_cost: ctx.weight(WeightNo::SignalWriteAfterRead),
            load_data_cost: ctx.weight(WeightNo::LoadPageDataFromStorage),
        };
        Some((GasLeft { gas, allowance }, gas_left_charger))
    } else {
        None
    };

    let handler = SignalAccessHandler { is_write, gas_ctx };
    process::process_lazy_pages(ctx, handler, page)
}

/// User signal handler. Logic can depends on lazy-pages version.
/// See also "user_signal_handler_internal".
pub(crate) unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let ctx = ctx.execution_context_mut()?;
        user_signal_handler_internal(ctx, info)
    })
}

struct SignalAccessHandler {
    is_write: bool,
    gas_ctx: Option<(GasLeft, GasLeftCharger)>,
}

impl AccessHandler for SignalAccessHandler {
    type Pages = GearPageNumber;
    type Output = ();

    fn is_write(&self) -> bool {
        self.is_write
    }

    fn last_page(page: &Self::Pages) -> Option<GearPageNumber> {
        Some(*page)
    }

    fn check_status_is_gas_exceeded() -> Result<(), Error> {
        // Because we unprotect all lazy-pages when status is `exceeded`, then
        // we cannot receive signals from wasm memory until the end of execution.
        Err(Error::SignalWhenStatusGasExceeded)
    }

    fn check_stack_memory_access() -> Result<(), Error> {
        // Stack memory is always unprotected, so we cannot receive signal from it.
        Err(Error::SignalFromStackMemory)
    }

    fn check_write_accessed_memory_access() -> Result<(), Error> {
        // Write accessed memory is unprotected, so signal cannot be received from it.
        Err(Error::SignalFromWriteAccessedPage)
    }

    fn check_read_from_accessed_memory() -> Result<(), Error> {
        // Accessed memory is not read protected,
        // so read memory access signal cannot be received from it.
        Err(Error::ReadAccessSignalFromAccessedPage)
    }

    fn charge_for_page_access(
        &mut self,
        page: GearPageNumber,
        is_accessed: bool,
    ) -> Result<Status, Error> {
        let (gas_left, gas_left_charger) = match self.gas_ctx.as_mut() {
            Some(ctx) => ctx,
            None => return Ok(Status::Normal),
        };
        gas_left_charger.charge_for_page_access(gas_left, page, self.is_write, is_accessed)
    }

    fn charge_for_page_data_loading(&mut self) -> Result<Status, Error> {
        let (gas_left, gas_left_charger) = match self.gas_ctx.as_mut() {
            Some(ctx) => ctx,
            None => return Ok(Status::Normal),
        };
        Ok(gas_left_charger.charge_for_page_data_load(gas_left))
    }

    fn process_pages(
        page: GearPageNumber,
        mut process_one: impl FnMut(GearPageNumber) -> Result<(), Error>,
    ) -> Result<(), Error> {
        process_one(page)
    }

    fn into_output(self, ctx: &mut LazyPagesExecutionContext) -> Result<Self::Output, Error> {
        if let (Some((gas_left, _)), Some(globals_config)) =
            (self.gas_ctx, ctx.globals_context.as_ref())
        {
            unsafe {
                globals::apply_for_global(globals_config, GlobalNo::GasLimit, |_| {
                    Ok(Some(gas_left.gas))
                })?;
                globals::apply_for_global(globals_config, GlobalNo::AllowanceLimit, |_| {
                    Ok(Some(gas_left.allowance))
                })?;
            }
        }
        Ok(())
    }
}
