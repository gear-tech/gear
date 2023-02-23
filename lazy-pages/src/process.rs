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

use std::cell::RefMut;

use gear_backend_common::lazy_pages::Status;
use gear_core::memory::{GearPage, GranularityPage, PageU32Size, PagesIterInclusive};

use crate::{
    common::{Error, LazyPage, LazyPagesExecutionContext, PagePrefix},
    mprotect,
};

/// `process_lazy_pages` use struct which implements this trait,
/// to process in custom logic two cases: host function call and signal.
pub(crate) trait AccessHandler {
    type Pages;
    type Output;

    /// Returns wether it is write access
    fn is_write(&self) -> bool;

    /// Returns whether gas exceeded status is allowed for current access.
    fn check_status_is_gas_exceeded() -> Result<(), Error>;

    /// Returns whether stack memory access can appear for the case.
    fn check_stack_memory_access() -> Result<(), Error>;

    /// Returns whether released memory access is allowed for the case.
    fn check_released_memory_access() -> Result<(), Error>;

    /// Returns wether already accessed memory read access is allowed for the case.
    fn check_read_from_accessed_memory() -> Result<(), Error>;

    /// Charge for accessed pages.
    fn charge_for_pages(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        pages: PagesIterInclusive<LazyPage>,
    ) -> Result<Status, Error>;

    /// Charge for one granularity page data loading.
    fn charge_for_data_loading(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        page: GranularityPage,
    ) -> Result<Status, Error>;

    /// Get the biggest page from `pages`.
    fn last_page(pages: &Self::Pages) -> Option<LazyPage>;

    /// Apply `f` for all `pages`.
    fn apply_for_pages(
        pages: Self::Pages,
        f: impl FnMut(PagesIterInclusive<LazyPage>) -> Result<(), Error>,
    ) -> Result<(), Error>;

    /// Drops and returns output.
    fn into_output(
        self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
    ) -> Result<Self::Output, Error>;
}

/// Load data for `page` from storage.
unsafe fn load_data_for_page(
    wasm_mem_addr: usize,
    prefix: &mut PagePrefix,
    page: LazyPage,
) -> Result<(), Error> {
    for gear_page in page.to_pages_iter::<GearPage>() {
        let page_buffer_ptr = (wasm_mem_addr as *mut u8).add(gear_page.offset() as usize);
        let buffer_as_slice =
            std::slice::from_raw_parts_mut(page_buffer_ptr, GearPage::size() as usize);
        let res = sp_io::storage::read(prefix.calc_key_for_page(gear_page), buffer_as_slice, 0);

        log::trace!("{:?} has data in storage: {}", gear_page, res.is_some());

        // Check data size is valid.
        if let Some(size) = res.filter(|&size| size != GearPage::size()) {
            return Err(Error::InvalidPageDataSize {
                expected: GearPage::size(),
                actual: size,
            });
        }
    }
    Ok(())
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
///
/// [PAGE_STORAGE_GRANULARITY] (PSG) case - if page is write accessed
/// first time in program live, then this page has no data in storage yet.
/// This also means that all pages from the same PSG interval has no data in storage.
/// So, in this case we have to insert in `released_pages` all pages from the same
/// PSG interval, in order to upload their data to storage later in runtime.
/// We have to make separate logic for this case in order to support consensus
/// between nodes with different native page sizes. For example, if one node
/// has native page size 4kBit and other 16kBit, then (without PSG logic)
/// for first one gear page will be uploaded and for second 4 gear pages.
/// This can cause conflicts in data about pages that have data in storage.
/// So, to avoid this we upload all pages from PSG interval (which is 16kBit now),
/// and restrict to run node on machines, that have native page number bigger than PSG.
pub(crate) unsafe fn process_lazy_pages<H: AccessHandler>(
    mut ctx: RefMut<LazyPagesExecutionContext>,
    mut handler: H,
    pages: H::Pages,
) -> Result<H::Output, Error> {
    let wasm_mem_size = ctx.wasm_mem_size.ok_or(Error::WasmMemSizeIsNotSet)?;

    if let Some(last_page) = H::last_page(&pages) {
        // Check that all pages are inside wasm memory.
        if last_page.end_offset() >= wasm_mem_size.offset() {
            return Err(Error::OutOfWasmMemoryAccess);
        }
    } else {
        // Accessed pages are empty - nothing to do.
        return handler.into_output(&mut ctx);
    }

    let status = ctx.status.as_ref().ok_or(Error::StatusIsNone)?;
    match status {
        Status::Normal => {}
        Status::GasLimitExceeded | Status::GasAllowanceExceeded => {
            H::check_status_is_gas_exceeded()?;
            return handler.into_output(&mut ctx);
        }
    }

    let stack_end = ctx.stack_end_wasm_page;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
    let mut prefix = PagePrefix::new_from_program_prefix(
        ctx.program_storage_prefix
            .as_ref()
            .ok_or(Error::ProgramPrefixIsNotSet)?,
    );

    // Returns `true` if new status is not `Normal`.
    let update_status = |ctx: &mut RefMut<LazyPagesExecutionContext>, status| {
        ctx.status.replace(status);

        // If new status is not [Status::Normal], then unprotect lazy-pages
        // and continue work until the end of current wasm block. We don't care
        // about future contract execution correctness, because gas limit or allowance exceed.
        match status {
            Status::Normal => Ok(false),
            Status::GasLimitExceeded | Status::GasAllowanceExceeded => {
                log::trace!(
                    "Gas limit or allowance exceed, so removes protection from all wasm memory \
                    and continues execution until the end of current wasm block"
                );
                mprotect::mprotect_interval(
                    wasm_mem_addr,
                    wasm_mem_size.offset() as usize,
                    true,
                    true,
                )
                .map(|_| true)
            }
        }
    };

    let f = |pages: PagesIterInclusive<LazyPage>| {
        let status = handler.charge_for_pages(&mut ctx, pages.clone())?;
        if update_status(&mut ctx, status)? {
            return Ok(());
        }

        for lazy_page in pages {
            let granularity_page = lazy_page.to_page();
            if lazy_page.offset() < stack_end.offset() {
                // Nothing to do, page has r/w accesses and data is in correct state.
                H::check_stack_memory_access()?;
            } else if ctx.released_pages.contains(&lazy_page) {
                // Nothing to do, page has r/w accesses and data is in correct state.
                H::check_released_memory_access()?;
            } else if ctx.accessed_pages.contains(&lazy_page) {
                if handler.is_write() {
                    // Set read/write access for page and add page to released.
                    mprotect::mprotect_interval(
                        wasm_mem_addr + lazy_page.offset() as usize,
                        LazyPage::size() as usize,
                        true,
                        true,
                    )?;
                    ctx.add_to_released(lazy_page)?;
                } else {
                    // Nothing to do, page has read accesses and data is in correct state.
                    H::check_read_from_accessed_memory()?;
                }
            } else {
                let unprotected =
                    if sp_io::storage::exists(prefix.calc_key_for_page(lazy_page.to_page())) {
                        // Need to set read/write access.
                        mprotect::mprotect_interval(
                            wasm_mem_addr + lazy_page.offset() as usize,
                            LazyPage::size() as usize,
                            true,
                            true,
                        )?;
                        let status = handler.charge_for_data_loading(&mut ctx, granularity_page)?;
                        if update_status(&mut ctx, status)? {
                            return Ok(());
                        }
                        load_data_for_page(wasm_mem_addr, &mut prefix, lazy_page)?;
                        true
                    } else {
                        false
                    };

                // Add `lazy_page` to accessed pages.
                ctx.accessed_pages.insert(lazy_page);

                if handler.is_write() {
                    if !unprotected {
                        mprotect::mprotect_interval(
                            wasm_mem_addr + lazy_page.offset() as usize,
                            LazyPage::size() as usize,
                            true,
                            true,
                        )?;
                    }
                    ctx.add_to_released(lazy_page)?;
                } else {
                    // Set only read access for page.
                    mprotect::mprotect_interval(
                        wasm_mem_addr + lazy_page.offset() as usize,
                        LazyPage::size() as usize,
                        true,
                        false,
                    )?;
                }
            }
        }

        Ok(())
    };

    H::apply_for_pages(pages, f)?;

    handler.into_output(&mut ctx)
}
