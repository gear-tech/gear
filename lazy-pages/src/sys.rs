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

//! Lazy pages signal handler functionality.

use cfg_if::cfg_if;
use region::Protection;
use std::cell::RefMut;

use crate::{Error, LazyPagesExecutionContext, WasmAddr, LAZY_PAGES_CONTEXT};

use gear_core::memory::{PageNumber, PAGE_STORAGE_GRANULARITY};

// These constants are used both in runtime and in lazy-pages backend,
// so we make here additional checks. If somebody would change these values
// in runtime, then he also should pay attention to support new values here:
// 1) must rebuild node after that.
// 2) must support old runtimes: need to make lazy pages version with old constants values.
static_assertions::const_assert_eq!(PageNumber::size(), 0x1000);
static_assertions::const_assert_eq!(PAGE_STORAGE_GRANULARITY, 0x4000);

cfg_if! {
    if #[cfg(windows)] {
        mod windows;
        pub(crate) use windows::*;
    } else if #[cfg(unix)] {
        mod unix;
        pub(crate) use unix::*;
    } else {
        compile_error!("lazy pages are not supported on your system. Disable `lazy-pages` feature");
    }
}

pub trait UserSignalHandler {
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
            .copy_from_slice(page.0.to_le_bytes().as_slice());
        &self.buffer
    }
}

/// Wasm address wrapper, for which we can be sure
/// it's in safe/checked state, so we can avoid some
/// checks while using this addr.
struct CheckedWasmAddr {
    wasm_mem_addr: usize,
    wasm_mem_size: u32,
    addr: WasmAddr,
}

impl CheckedWasmAddr {
    pub fn new_from_native(
        native_addr: usize,
        wasm_mem_addr: usize,
        stack_end: WasmAddr,
        wasm_mem_size: u32,
    ) -> Result<Self, Error> {
        let wasm_mem_end_addr = wasm_mem_addr
            .checked_add(wasm_mem_size as usize)
            .ok_or(Error::AddrArithOverflow)?;

        let addr =
            native_addr
                .checked_sub(wasm_mem_addr)
                .ok_or(Error::SignalFromUnknownMemory {
                    addr: native_addr,
                    wasm_mem_addr,
                    wasm_mem_end_addr,
                })?;

        if addr >= wasm_mem_size as usize {
            return Err(Error::SignalFromUnknownMemory {
                addr: native_addr,
                wasm_mem_addr,
                wasm_mem_end_addr,
            });
        }

        // `addr` is less then `wasm_mem_size`, so `as u32` is safe.
        let addr = addr as u32;

        if addr < stack_end {
            return Err(Error::SignalFromStackMemory {
                wasm_addr: addr,
                stack_end,
            });
        }

        Ok(Self {
            wasm_mem_addr,
            wasm_mem_size,
            addr,
        })
    }

    pub fn as_page_number(&self) -> PageNumber {
        (self.addr / PageNumber::size() as u32).into()
    }

    pub fn as_native_addr(&self) -> usize {
        // no need to check `+` because we check this in `Self::new_from_native`.
        // `addr` can be only decreased and `wasm_mem_addr` is never changed.
        self.wasm_mem_addr + self.addr as usize
    }

    pub fn align_down(&mut self, alignment: u32) {
        self.addr = (self.addr / alignment) * alignment;
    }

    /// Checks that interval [`addr`, `addr` + `size`) is in wasm memory.
    pub fn check_interval(&self, size: u32) -> Result<(), Error> {
        // `addr` is in wasm mem, so `sub` is safe.
        (size <= self.wasm_mem_size - self.addr)
            .then_some(())
            .ok_or_else(|| Error::AccessedIntervalNotLiesInWasmBuffer {
                begin_addr: self.as_native_addr(),
                end_addr: self.as_native_addr() + size as usize,
                wasm_mem_addr: self.wasm_mem_addr,
                wasm_mem_end_addr: self.wasm_mem_addr + self.wasm_mem_size as usize,
            })
    }

    /// Get raw addr in wasm memory
    pub fn get(&self) -> WasmAddr {
        self.addr
    }
}

pub struct DefaultUserSignalHandler;

impl UserSignalHandler for DefaultUserSignalHandler {
    unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
        user_signal_handler(info)
    }
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
    mut ctx: RefMut<LazyPagesExecutionContext>,
    info: ExceptionInfo,
) -> Result<(), Error> {
    // We use here `u32` as type for sizes, because wasm memory is 32-bits.
    // Native page size cannot be bigger than PSG (see `crate::init`), so `as u32` is safe.
    let native_ps = region::page::size() as u32;
    let gear_ps = PageNumber::size() as u32;
    let psg = PAGE_STORAGE_GRANULARITY as u32;
    let lazy_page_size = native_ps.max(gear_ps);
    let num_of_gear_pages_in_one_lazy = lazy_page_size / gear_ps;

    let native_addr = info.fault_addr as usize;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)? as usize;
    let wasm_mem_size = ctx.wasm_mem_size.ok_or(Error::WasmMemSizeIsNotSet)?;
    let stack_end = ctx.stack_end_wasm_addr;
    let mut prefix = PagePrefix::new_from_program_prefix(
        ctx.program_storage_prefix
            .as_ref()
            .ok_or(Error::ProgramPrefixIsNotSet)?,
    );

    let mut wasm_addr =
        CheckedWasmAddr::new_from_native(native_addr, wasm_mem_addr, stack_end, wasm_mem_size)?;

    // Wasm addr of native page, which contains accessed gear page or which is in the beginning
    // of the accessed gear page, if native page size is smaller then gear page size.
    wasm_addr.align_down(lazy_page_size);

    // If `is_write` is Some, than we definitely know whether it's `write` or `read` access.
    // In other case we handle first access as it's `read`.
    // This also means that we will set read protection for the accessed interval after handling.
    // If in reality it's `write` access, then right after return from signal handler,
    // another signal will appear from the same instruction and for the same address.
    // Because we insert accessed pages in `accessed_pages_addrs`, then handling second signal
    // we can definitely identify that this signal from `write` access.
    let is_definitely_write =
        ctx.accessed_pages_addrs.contains(&wasm_addr.get()) || info.is_write.unwrap_or(false);

    let is_psg_case = is_definitely_write
        && native_ps < psg
        && !sp_io::storage::exists(prefix.calc_key_for_page(wasm_addr.as_page_number()));
    let unprot_size = if is_psg_case {
        log::trace!("is PSG case - we need to upload to storage data for all pages from `PAGE_STORAGE_GRANULARITY`");
        wasm_addr.align_down(psg);
        psg
    } else {
        native_ps
    };

    wasm_addr.check_interval(unprot_size)?;

    // Set r/w protection in order to load data from storage into mem buffer.
    let unprot_addr = wasm_addr.as_native_addr();
    log::trace!("mprotect r/w, addr = {unprot_addr:#x}, size = {unprot_size:#x}");
    region::protect(
        unprot_addr as *mut (),
        unprot_size as usize,
        Protection::READ_WRITE,
    )?;

    let fist_gear_page = wasm_addr.as_page_number();

    for idx in 0..unprot_size / lazy_page_size {
        // Arithmetic operations are safe here, because this values represents, address and
        // pages, for which we have already checked, that they are inside wasm memory.
        let lazy_page_wasm_addr = wasm_addr.get() + idx * lazy_page_size;
        let begin = fist_gear_page.0 + idx * num_of_gear_pages_in_one_lazy;
        let end = begin + num_of_gear_pages_in_one_lazy;

        if ctx.accessed_pages_addrs.contains(&lazy_page_wasm_addr) {
            log::trace!("lazy page {lazy_page_wasm_addr:#x} is already accessed");
            for gear_page in (begin..end).map(PageNumber) {
                log::trace!("add {gear_page:?} to released");
                if !ctx.released_pages.insert(gear_page) {
                    return Err(Error::DoubleRelease(gear_page));
                }
            }
            continue;
        }

        ctx.accessed_pages_addrs.insert(lazy_page_wasm_addr);

        for gear_page in (begin..end).map(PageNumber) {
            let page_buffer_ptr = (wasm_mem_addr as *mut u8).add(gear_page.offset());
            let buffer_as_slice = std::slice::from_raw_parts_mut(page_buffer_ptr, gear_ps as usize);
            let res = sp_io::storage::read(prefix.calc_key_for_page(gear_page), buffer_as_slice, 0);

            log::trace!("{:?} has data in storage: {}", gear_page, res.is_some());

            if let Some(size) = res.filter(|&size| size as usize != PageNumber::size()) {
                return Err(Error::InvalidPageDataSize {
                    expected: PageNumber::size(),
                    actual: size,
                });
            }

            if is_definitely_write {
                log::trace!("add {gear_page:?} to released");
                if !ctx.released_pages.insert(gear_page) {
                    return Err(Error::DoubleRelease(gear_page));
                }
            }
        }
    }

    if !is_definitely_write {
        log::trace!("Is first access - set read prot");
        region::protect(
            unprot_addr as *mut (),
            unprot_size as usize,
            Protection::READ,
        )?;
    } else {
        log::trace!("Is write access - keep r/w prot");
    }

    Ok(())
}

/// User signal handler. Logic depends on lazy pages version.
/// For the most recent logic see "self::user_signal_handler_internal"
pub unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_CONTEXT.with(|ctx| user_signal_handler_internal(ctx.borrow_mut(), info))
}
