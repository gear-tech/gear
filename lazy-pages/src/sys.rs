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

//! Lazy-pages signal handler functionality.

use cfg_if::cfg_if;
use region::Protection;
use std::{
    cell::RefMut,
    collections::BTreeSet,
    convert::{TryFrom, TryInto},
    iter::FromIterator,
};

use crate::{utils, Error, LazyPage, LazyPagesExecutionContext, LAZY_PAGES_CONTEXT};

use gear_core::memory::{
    PageNumber, PageU32Size, PagesIterInclusive, GEAR_PAGE_SIZE, PAGE_STORAGE_GRANULARITY,
};

// These constants are used both in runtime and in lazy-pages backend,
// so we make here additional checks. If somebody would change these values
// in runtime, then he also should pay attention to support new values here:
// 1) must rebuild node after that.
// 2) must support old runtimes: need to make lazy-pages version with old constants values.
static_assertions::const_assert_eq!(GEAR_PAGE_SIZE, 0x1000);
static_assertions::const_assert_eq!(PAGE_STORAGE_GRANULARITY, 0x4000);

cfg_if! {
    if #[cfg(windows)] {
        mod windows;
        pub(crate) use windows::*;
    } else if #[cfg(unix)] {
        mod unix;
        pub(crate) use unix::*;
    } else {
        compile_error!("lazy-pages are not supported on your system. Disable `lazy-pages` feature");
    }
}

pub(crate) trait UserSignalHandler {
    /// # Safety
    ///
    /// It's expected handler calls sys-calls to protect memory
    unsafe fn handle(info: ExceptionInfo) -> Result<(), Error>;
}

#[derive(Debug)]
pub struct ExceptionInfo {
    /// Address where fault is occurred
    pub fault_addr: *const (),
    pub is_write: Option<bool>,
}

/// Struct for fast calculation of page key in storage.
/// Key consists of two parts:
/// 1) current program prefix in storage
/// 2) page number in little endian bytes order
/// First part is always the same, so we can copy it to buffer
/// once and then use it for all pages.
struct PagePrefix {
    buffer: Vec<u8>,
}

impl PagePrefix {
    /// New page prefix from program prefix
    pub fn new_from_program_prefix(program_prefix: &[u8]) -> Self {
        Self {
            buffer: [program_prefix, &u32::MAX.to_le_bytes()].concat(),
        }
    }
    /// Returns key in storage for `page`.
    pub fn calc_key_for_page(&mut self, page: PageNumber) -> &[u8] {
        let len = self.buffer.len();
        self.buffer[len - std::mem::size_of::<u32>()..len]
            .copy_from_slice(page.raw().to_le_bytes().as_slice());
        &self.buffer
    }
}

pub struct DefaultUserSignalHandler;

impl UserSignalHandler for DefaultUserSignalHandler {
    unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
        user_signal_handler(info)
    }
}

pub(crate) unsafe fn process_lazy_pages(
    mut ctx: RefMut<LazyPagesExecutionContext>,
    accessed_pages: BTreeSet<LazyPage>,
    is_write: bool,
    is_signal: bool,
) -> Result<(), Error> {
    let wasm_mem_size = ctx.wasm_mem_size.ok_or(Error::WasmMemSizeIsNotSet)?;

    if let Some(last_page) = accessed_pages.last() {
        // Check that all pages are inside wasm memory.
        if last_page.end_offset() >= wasm_mem_size.offset() {
            return Err(Error::OutOfWasmMemoryAccess);
        }
    } else {
        // Accessed pages are empty - nothing to do.
        return Ok(());
    }

    let stack_end = ctx.stack_end_wasm_page;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
    let mut prefix = PagePrefix::new_from_program_prefix(
        ctx.program_storage_prefix
            .as_ref()
            .ok_or(Error::ProgramPrefixIsNotSet)?,
    );

    let f = |pages: PagesIterInclusive<LazyPage>| {
        let psg = PAGE_STORAGE_GRANULARITY as u32;
        let mut start = if let Some(start) = pages.current() {
            start
        } else {
            // Interval is empty, so nothing to process.
            return Ok(());
        };
        let mut end = pages.end();

        // Extend pages interval, if start or end access pages, which has no data in storage.
        if is_write && LazyPage::size() < psg {
            if !sp_io::storage::exists(prefix.calc_key_for_page(start.to_page())) {
                start = start.align_down(psg.try_into().expect("Cannot be null"));
            }
            if !sp_io::storage::exists(prefix.calc_key_for_page(end.to_page())) {
                // Make page end aligned to `psg` for `end`.
                // This operations are safe, because `psg` is power of two and smaller then `u32::MAX`.
                // `LazyPage::size()` is less or equal then `psg` and `psg % LazyPage::size() == 0`.
                end = LazyPage::from_offset((end.offset() / psg) * psg + (psg - LazyPage::size()));
            }
        }

        let pages = start.iter_end_inclusive(end).unwrap_or_else(|err| {
            unreachable!("`start` can be only decreased, `end` can be only increased, so `start` <= `end`, but get: {}", err)
        });

        for lazy_page in pages {
            if lazy_page.offset() < stack_end.offset() {
                // Nothing to do, page has r/w accesses and data is in correct state.
                if is_signal {
                    return Err(Error::SignalFromStackMemory);
                }
            } else if ctx.released_pages.contains(&lazy_page) {
                // Nothing to do, page has r/w accesses and data is in correct state.
                if is_signal {
                    return Err(Error::SignalFromReleasedPage);
                }
            } else if ctx.accessed_lazy_pages.contains(&lazy_page) {
                if is_write {
                    // Set read/write access for page and add page to released.
                    region::protect(
                        (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                        LazyPage::size() as usize,
                        Protection::READ_WRITE,
                    )?;
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                } else {
                    // Nothing to do, page has read accesses and data is in correct state.
                    if is_signal {
                        return Err(Error::ReadAccessSignalFromAccessedPage);
                    }
                }
            } else {
                // Need to set read/write access,
                // download data for `lazy_page` from storage and add `lazy_page` to accessed pages.
                region::protect(
                    (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                    LazyPage::size() as usize,
                    Protection::READ_WRITE,
                )?;

                for gear_page in lazy_page.to_pages_iter::<PageNumber>() {
                    let page_buffer_ptr =
                        (wasm_mem_addr as *mut u8).add(gear_page.offset() as usize);
                    let buffer_as_slice = std::slice::from_raw_parts_mut(
                        page_buffer_ptr,
                        PageNumber::size() as usize,
                    );
                    let res = sp_io::storage::read(
                        prefix.calc_key_for_page(gear_page),
                        buffer_as_slice,
                        0,
                    );

                    log::trace!("{:?} has data in storage: {}", gear_page, res.is_some());

                    // Check data size is valid.
                    if let Some(size) = res.filter(|&size| size != PageNumber::size()) {
                        return Err(Error::InvalidPageDataSize {
                            expected: PageNumber::size(),
                            actual: size,
                        });
                    }
                }

                ctx.accessed_lazy_pages.insert(lazy_page);

                if is_write {
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                } else {
                    // Set only read access for page.
                    region::protect(
                        (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                        LazyPage::size() as usize,
                        Protection::READ,
                    )?;
                }
            }
        }

        Ok(())
    };

    utils::with_inclusive_ranges(&accessed_pages, f)
}

/// Before contract execution some pages from wasm memory buffer have been protected.
/// When wasm executer tries to access one of these pages,
/// OS emits sigsegv or sigbus or EXCEPTION_ACCESS_VIOLATION.
/// This function handles the signal.
/// Using OS signal info, it identifies memory location and page,
/// which emits the signal. It removes read and write protections for page,
/// then it loads wasm page data from storage to wasm page memory location.
/// If native page size is bigger than gear page size, then this will be done
/// for all gear pages from accessed native page.
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
///
/// After signal handler is done, OS returns execution to the same machine
/// instruction, which cause signal. Now memory which this instruction accesses
/// is not protected and with correct data.
unsafe fn user_signal_handler_internal(
    ctx: RefMut<LazyPagesExecutionContext>,
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
    let accessed_pages = BTreeSet::from_iter(std::iter::once(lazy_page));
    process_lazy_pages(ctx, accessed_pages, is_write, true)
}

/// User signal handler. Logic can depends on lazy-pages version.
/// For the most recent logic see "self::user_signal_handler_internal"
pub(crate) unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_CONTEXT.with(|ctx| user_signal_handler_internal(ctx.borrow_mut(), info))
}
