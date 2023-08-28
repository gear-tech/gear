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

//! Lazy-pages memory accesses processing main logic.

use crate::{
    common::{Error, LazyPagesExecutionContext},
    mprotect,
};
use gear_backend_common::lazy_pages::Status;
use gear_core::pages::{GearPage, PageDynSize};
use std::slice;

/// `process_lazy_pages` use struct which implements this trait,
/// to process in custom logic two cases: host function call and signal.
pub(crate) trait AccessHandler {
    type Pages;
    type Output;

    /// Returns whether it is write access
    fn is_write(&self) -> bool;

    /// Returns whether gas exceeded status is allowed for current access.
    fn check_status_is_gas_exceeded() -> Result<(), Error>;

    /// Returns whether stack memory access can appear for the case.
    fn check_stack_memory_access() -> Result<(), Error>;

    /// Returns whether write accessed memory access is allowed for the case.
    fn check_write_accessed_memory_access() -> Result<(), Error>;

    /// Returns whether already accessed memory read access is allowed for the case.
    fn check_read_from_accessed_memory() -> Result<(), Error>;

    /// Charge for accessed gear page.
    fn charge_for_page_access(
        &mut self,
        page: GearPage,
        is_already_accessed: bool,
    ) -> Result<Status, Error>;

    /// Charge for one gear page data loading.
    fn charge_for_page_data_loading(&mut self) -> Result<Status, Error>;

    /// Apply `f` for all `pages`.
    fn process_pages(
        pages: Self::Pages,
        process_one: impl FnMut(GearPage) -> Result<(), Error>,
    ) -> Result<(), Error>;

    /// Drops and returns output.
    fn into_output(self, ctx: &mut LazyPagesExecutionContext) -> Result<Self::Output, Error>;
}

/// Lazy-pages accesses processing main function.
/// Acts differently for signals and host functions accesses,
/// but main logic is the same:
/// It removes read and write protections for page,
/// then it loads wasm page data from storage to wasm page memory location.
/// If native page size is bigger than gear page size, then this will be done
/// for all gear pages from accessed native page.
/// 1) Set new access pages protection accordingly it is read or write access.
/// 2) If some page contains data in storage, then load this data and place it in
/// program's wasm memory.
/// 3) Charge gas for access and data loading.
pub(crate) fn process_lazy_pages<H: AccessHandler>(
    ctx: &mut LazyPagesExecutionContext,
    mut handler: H,
    pages: H::Pages,
    page_size: u32,
) -> Result<H::Output, Error> {
    let wasm_mem_size = ctx.wasm_mem_size.offset(ctx);

    if ctx.status != Status::Normal {
        H::check_status_is_gas_exceeded()?;
        return handler.into_output(ctx);
    }

    let stack_end = ctx.stack_end;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;

    // Returns `true` if new status is not `Normal`.
    let update_status = |ctx: &mut LazyPagesExecutionContext, status| {
        ctx.status = status;

        // If new status is not [Status::Normal], then unprotect lazy-pages
        // and continue work until the end of current wasm block. We don't care
        // about future contract execution correctness, because gas limit or allowance exceed.
        match status {
            Status::Normal => Ok(false),
            Status::GasLimitExceeded => {
                log::trace!(
                    "Gas limit or allowance exceed, so removes protection from all wasm memory \
                    and continues execution until the end of current wasm block"
                );
                mprotect::mprotect_interval(wasm_mem_addr, wasm_mem_size as usize, true, true)
                    .map(|_| true)
            }
        }
    };

    let process_one = |page: GearPage| -> Result<(), Error> {
        if page.end_offset(ctx) >= wasm_mem_size {
            return Err(Error::OutOfWasmMemoryAccess);
        }
        let page_offset = page.offset(ctx);
        let page_buffer_ptr = unsafe { (wasm_mem_addr as *mut u8).add(page_offset as usize) };

        let protect_page = |prot_write| {
            mprotect::mprotect_interval(
                page_buffer_ptr as usize,
                page_size as usize,
                true,
                prot_write,
            )
        };

        match (
            page_offset < stack_end.offset(ctx),
            ctx.is_write_accessed(page),
            ctx.is_accessed(page),
        ) {
            (true, _, _) => H::check_stack_memory_access()?,
            (_, true, _) => H::check_write_accessed_memory_access()?,
            (_, _, true) => {
                if handler.is_write() {
                    let status = handler.charge_for_page_access(page, true)?;
                    if update_status(ctx, status)? {
                        return Ok(());
                    }
                    protect_page(true)?;
                    ctx.set_write_accessed(page)?;
                } else {
                    H::check_read_from_accessed_memory()?;
                }
            }
            _ => {
                let status = handler.charge_for_page_access(page, false)?;
                if update_status(ctx, status)? {
                    return Ok(());
                }
                let unprotected = if ctx.page_has_data_in_storage(page) {
                    let status = handler.charge_for_page_data_loading()?;
                    if update_status(ctx, status)? {
                        return Ok(());
                    }
                    protect_page(true)?;
                    let buffer_as_slice =
                        unsafe { slice::from_raw_parts_mut(page_buffer_ptr, page_size as usize) };
                    if !ctx.load_page_data_from_storage(page, buffer_as_slice)? {
                        unreachable!("`read` returns, that page has no data, but `exist` returns that there is one");
                    }
                    true
                } else {
                    false
                };
                ctx.set_accessed(page);
                if handler.is_write() {
                    if !unprotected {
                        protect_page(true)?;
                    }
                    ctx.set_write_accessed(page)?;
                } else {
                    protect_page(false)?;
                }
            }
        }

        Ok(())
    };

    H::process_pages(pages, process_one)?;

    handler.into_output(ctx)
}
