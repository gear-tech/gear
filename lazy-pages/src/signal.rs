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

use std::{cell::RefMut, convert::TryFrom};

use gear_backend_common::lazy_pages::Status;
use gear_core::{
    gas::GasLeft,
    memory::{GranularityPage, PageU32Size, PagesIterInclusive},
};

use crate::{
    common::{Error, LazyPage, LazyPagesExecutionContext},
    globals::{self, GearGlobal},
    process::{self, AccessHandler},
    utils, LAZY_PAGES_PROGRAM_CONTEXT,
};

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
    mut ctx: RefMut<LazyPagesExecutionContext>,
    info: ExceptionInfo,
) -> Result<(), Error> {
    let native_addr = info.fault_addr as usize;
    let is_write = info.is_write.ok_or(Error::ReadOrWriteIsUnknown)?;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;

    if native_addr < wasm_mem_addr {
        return Err(Error::OutOfWasmMemoryAccess);
    }

    let offset =
        u32::try_from(native_addr - wasm_mem_addr).map_err(|_| Error::OutOfWasmMemoryAccess)?;
    let lazy_page = LazyPage::from_offset(offset);

    let pages = if is_write {
        utils::handle_psg_case_one_page(&mut ctx, lazy_page)?
    } else {
        lazy_page.iter_once()
    };

    let gas_left = if let Some(globals_config) = ctx.globals_config.as_ref() {
        let gas = globals::apply_for_global(globals_config, GearGlobal::GasLimit, |_| Ok(None))?;
        let allowance =
            globals::apply_for_global(globals_config, GearGlobal::AllowanceLimit, |_| Ok(None))?;
        Some(GasLeft { gas, allowance })
    } else {
        None
    };

    let handler = SignalAccessHandler { is_write, gas_left };
    process::process_lazy_pages(ctx, handler, pages)
}

/// User signal handler. Logic can depends on lazy-pages version.
/// See also "user_signal_handler_internal".
pub(crate) unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| user_signal_handler_internal(ctx.borrow_mut(), info))
}

struct SignalAccessHandler {
    is_write: bool,
    gas_left: Option<GasLeft>,
}

impl SignalAccessHandler {
    fn sub_gas(gas_left: &mut GasLeft, amount: u64) -> Status {
        gas_left.gas = if let Some(gas) = gas_left.gas.checked_sub(amount) {
            gas
        } else {
            return Status::GasLimitExceeded;
        };
        gas_left.allowance = if let Some(gas) = gas_left.allowance.checked_sub(amount) {
            gas
        } else {
            return Status::GasAllowanceExceeded;
        };
        Status::Normal
    }
}

impl AccessHandler for SignalAccessHandler {
    type Pages = PagesIterInclusive<LazyPage>;
    type Output = ();

    fn is_write(&self) -> bool {
        self.is_write
    }

    fn last_page(pages: &Self::Pages) -> Option<LazyPage> {
        Some(pages.end())
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

    fn check_released_memory_access() -> Result<(), Error> {
        // Released memory is unprotected, so signal cannot be received from it.
        Err(Error::SignalFromReleasedPage)
    }

    fn check_read_from_accessed_memory() -> Result<(), Error> {
        // Accessed memory is not read protected,
        // so read memory access signal cannot be received from it.
        Err(Error::ReadAccessSignalFromAccessedPage)
    }

    fn charge_for_pages(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        pages: PagesIterInclusive<LazyPage>,
    ) -> Result<Status, Error> {
        let gas_left = if let Some(gas_left) = self.gas_left.as_mut() {
            gas_left
        } else {
            return Ok(Status::Normal);
        };

        let for_write = |ctx: &mut RefMut<LazyPagesExecutionContext>, page| {
            if ctx.set_write_charged(page) {
                if ctx.is_read_charged(page) {
                    ctx.lazy_pages_weights.signal_write_after_read.one()
                } else {
                    ctx.lazy_pages_weights.signal_write.one()
                }
            } else {
                0
            }
        };

        let for_read = |ctx: &mut RefMut<LazyPagesExecutionContext>, page| {
            if ctx.set_read_charged(page) {
                ctx.lazy_pages_weights.signal_read.one()
            } else {
                0
            }
        };

        let mut amount = 0u64;
        for page in pages.convert() {
            let amount_for_page = if self.is_write {
                for_write(ctx, page)
            } else {
                for_read(ctx, page)
            };
            amount = amount.saturating_add(amount_for_page);
        }

        Ok(Self::sub_gas(gas_left, amount))
    }

    fn charge_for_data_loading(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        page: GranularityPage,
    ) -> Result<Status, Error> {
        let gas_left = if let Some(gas_left) = self.gas_left.as_mut() {
            gas_left
        } else {
            return Ok(Status::Normal);
        };

        if ctx.set_load_data_charged(page) {
            Ok(Self::sub_gas(
                gas_left,
                ctx.lazy_pages_weights.load_page_storage_data.one(),
            ))
        } else {
            Ok(Status::Normal)
        }
    }

    fn apply_for_pages(
        pages: Self::Pages,
        mut f: impl FnMut(PagesIterInclusive<LazyPage>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        f(pages)
    }

    fn into_output(
        self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
    ) -> Result<Self::Output, Error> {
        if let Some(gas_left) = self.gas_left {
            if let Some(globals_config) = ctx.globals_config.as_ref() {
                unsafe {
                    globals::apply_for_global(globals_config, GearGlobal::GasLimit, |_| {
                        Ok(Some(gas_left.gas))
                    })?;
                    globals::apply_for_global(globals_config, GearGlobal::AllowanceLimit, |_| {
                        Ok(Some(gas_left.allowance))
                    })?;
                }
            }
        }
        Ok(())
    }
}
