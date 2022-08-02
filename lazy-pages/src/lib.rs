// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Lazy pages support.
//! In runtime data for contract wasm memory pages can be loaded in lazy manner.
//! All pages, which is supposed to be lazy, must be mprotected before contract execution.
//! During execution data from storage is loaded for all pages, which has been accesed.
//! See also `sys::user_signal_handler` in the source code.
//!
//! Currently we restrict twice write signal from same page during one execution.
//! It's not necessary behaviour, but more simple and safe.

// TODO: remove all deprecated code before release (issue #1147)
#![allow(useless_deprecated, deprecated)]

use gear_core::memory::{
    HostPointer, PageBuf, PageNumber, WasmPageNumber, PAGE_STORAGE_GRANULARITY,
};
use sp_std::vec::Vec;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

mod deprecated;
mod sys;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "WASM memory size is not set")]
    WasmMemSizeIsNotSet,
    #[display(fmt = "Overflow of addr arith operation")]
    AddrArithOverflow,
    #[display(fmt = "Program pages prefix in storage is not set")]
    ProgramPrefixIsNotSet,
    #[display(
        fmt = "Signal is from unknown memory: {:#x} not in [{:#x}, {:#x})",
        addr,
        wasm_mem_addr,
        wasm_mem_end_addr
    )]
    SignalFromUnknownMemory {
        addr: usize,
        wasm_mem_addr: usize,
        wasm_mem_end_addr: usize,
    },
    #[display(
        fmt = "Signal addr {:#x} is from wasm program virt-stack memory [0, {:#x})",
        wasm_addr,
        stack_end
    )]
    SignalFromStackMemory {
        wasm_addr: WasmAddr,
        stack_end: WasmAddr,
    },
    #[display(
        fmt = "Accessed pages are not lies in wasm memory: [{:#x}, {:#x}) not in [{:#x}, {:#x})",
        begin_addr,
        end_addr,
        wasm_mem_addr,
        wasm_mem_end_addr
    )]
    AccessedIntervalNotLiesInWasmBuffer {
        begin_addr: usize,
        end_addr: usize,
        wasm_mem_addr: usize,
        wasm_mem_end_addr: usize,
    },
    #[display(
        fmt = "Page data in storage must contain {} bytes, actually has {}",
        expected,
        actual
    )]
    InvalidPageDataSize { expected: usize, actual: u32 },
    /// Found a write signal from same page twice - see more in head comment.
    #[display(fmt = "Any page cannot be released twice: {:?}", _0)]
    DoubleRelease(PageNumber),
    #[display(fmt = "Protection error: {}", _0)]
    #[from]
    MemoryProtection(region::Error),

    #[deprecated]
    #[display(fmt = "Signal addr {:#x} is less then {:#x}", addr, wasm_mem_addr)]
    SignalAddrIsLessThenWasmMemAddr { addr: usize, wasm_mem_addr: usize },
    #[deprecated]
    #[display(fmt = "Exception is from unknown memory: addr = {:?}, {:?}", _0, _1)]
    LazyPageNotExistForSignalAddr(*const (), PageNumber),
}

pub(crate) type WasmAddr = u32;

#[derive(Clone, Copy)]
pub enum LazyPagesVersion {
    Version1,
    Version2,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct LazyPagesExecutionContext {
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: Option<u32>,
    /// Current program prefix in storage
    pub program_storage_prefix: Option<Vec<u8>>,
    /// Wasm addresses of lazy pages, that have been read or write accessed at least once.
    /// Lazy page here is page, which has `size = max(native_page_size, gear_page_size)`.
    pub accessed_pages_addrs: BTreeSet<WasmAddr>,
    /// End of stack wasm address. Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `released_lazy_pages`,
    /// which means that they will never be updated in storage.
    pub stack_end_wasm_addr: WasmAddr,
    /// Gear pages, which has been write accessed.
    pub released_lazy_pages: BTreeSet<PageNumber>,

    #[deprecated]
    /// Keys in storage for each lazy page.
    pub lazy_pages_info: BTreeMap<PageNumber, Vec<u8>>,
    #[deprecated]
    /// Released lazy pages and their data before execution.
    pub released_lazy_pages_old: BTreeMap<PageNumber, Option<PageBuf>>,
}

thread_local! {
    // NOTE: here we suppose, that each contract is executed in separate thread.
    // Or may be in one thread but consequentially.

    /// Identify whether signal handler is set for current thread.
    static LAZY_PAGES_ENABLED: RefCell<bool> = RefCell::new(false);
    /// Lazy pages impl version. Different runtimes may require different impl of lazy pages functionallity.
    static LAZY_PAGES_VERSION: RefCell<LazyPagesVersion> = RefCell::new(LazyPagesVersion::Version1);
    /// Lazy pages context for current execution.
    static LAZY_PAGES_CONTEXT: RefCell<LazyPagesExecutionContext> = RefCell::new(Default::default());
}

#[deprecated]
pub fn get_lazy_pages_numbers() -> Vec<PageNumber> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().lazy_pages_info.keys().copied().collect())
}

pub fn initilize_for_program(
    wasm_mem_addr: Option<HostPointer>,
    wasm_mem_size: u32,
    stack_end_wasm_addr: Option<WasmAddr>,
    program_prefix: Vec<u8>,
) -> Result<(), Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.accessed_pages_addrs.clear();
        ctx.released_lazy_pages.clear();

        assert_eq!(
            wasm_mem_addr
                .map(|addr| addr as usize % region::page::size())
                .unwrap_or(0),
            0
        );
        ctx.wasm_mem_addr = wasm_mem_addr;

        assert_eq!(wasm_mem_size as usize % WasmPageNumber::size(), 0);
        ctx.wasm_mem_size = Some(wasm_mem_size);

        if let Some(stack_end_wasm_addr) = stack_end_wasm_addr {
            assert_eq!(stack_end_wasm_addr as usize % WasmPageNumber::size(), 0);
            assert!(stack_end_wasm_addr <= wasm_mem_size);
            ctx.stack_end_wasm_addr = stack_end_wasm_addr;
        } else {
            ctx.stack_end_wasm_addr = 0;
        }

        ctx.program_storage_prefix = Some(program_prefix);
    });
    Ok(())
}

/// Set end of stack addr in wasm memory.
pub fn set_stack_end_wasm_addr(stack_end_wasm_addr: WasmAddr) {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow_mut().stack_end_wasm_addr = stack_end_wasm_addr);
}

/// Returns end of stack address in wasm memory.
pub fn get_stack_end_wasm_addr() -> WasmAddr {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().stack_end_wasm_addr)
}

/// Returns current wasm mem buffer pointer, if it's set.
pub fn get_wasm_mem_addr() -> Option<HostPointer> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_addr)
}

/// Returns current wasm mem buffer size, if it's set
pub fn get_wasm_mem_size() -> Option<u32> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_size)
}

/// Set current wasm memory begin addr in global context
pub fn set_wasm_mem_begin_addr(wasm_mem_addr: HostPointer) {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_addr.insert(wasm_mem_addr);
    });
}

/// Set current wasm memory size in global context
pub fn set_wasm_mem_size(wasm_mem_size: u32) {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_size.insert(wasm_mem_size);
    });
}

/// Reset lazy pages info
pub fn reset_context() {
    LAZY_PAGES_CONTEXT.with(|ctx| *ctx.borrow_mut() = Default::default());
}

/// Returns vec of lazy pages which has been accessed
pub fn get_released_pages() -> Vec<PageNumber> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().released_lazy_pages.iter().copied().collect())
}

/// Returns whether lazy pages env is enabled
pub fn is_enabled() -> bool {
    LAZY_PAGES_ENABLED.with(|x| *x.borrow())
}

#[deprecated]
/// Set storage `key` for `page` in global context
pub fn set_lazy_page_info(page: PageNumber, key: &[u8]) {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let page_end = (page.offset() + PageNumber::size()) as u32;
        let need_replace_size = ctx
            .wasm_mem_size
            .map(|size| size < page_end)
            .unwrap_or(true);
        if need_replace_size {
            let _ = ctx.wasm_mem_size.insert(page_end);
        }
        ctx.lazy_pages_info.insert(page, key.to_vec());
    });
}

#[deprecated]
/// Set lazy pages info and program `pages` `prefix` in global context
pub fn append_lazy_pages_info(pages: Vec<u32>, prefix: Vec<u8>) {
    let max_page = pages.iter().max().copied().unwrap_or(0);
    let end_offset = ((max_page + 1) as usize * PageNumber::size()) as u32;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();

        ctx.lazy_pages_info
            .extend(pages.iter().map(|&p| (PageNumber(p), Vec::default())));

        let need_replace_size = ctx
            .wasm_mem_size
            .map(|size| size < end_offset)
            .unwrap_or(true);
        if need_replace_size {
            let _ = ctx.wasm_mem_size.insert(end_offset);
        }
        let _ = ctx.program_storage_prefix.insert(prefix);
    });
}

/// Set program pages `prefix` in storage in global context
pub fn set_program_prefix(prefix: Vec<u8>) {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().program_storage_prefix.insert(prefix);
    });
}

/// Returns data for released `page`
#[deprecated]
pub fn get_released_page_data(page: PageNumber) -> Option<PageBuf> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow_mut()
            .released_lazy_pages_old
            .get_mut(&page)?
            .take()
    })
}

#[derive(Debug, derive_more::Display)]
pub enum InitError {
    #[display(fmt = "Initial context is not in default state before initialization")]
    InitialContextIsNotDefault,
    #[display(fmt = "Native page size {:#x} is not suitable for lazy pages", _0)]
    NativePageSizeIsNotSuitable(usize),
    #[display(fmt = "Can not set signal handler: {}", _0)]
    CanNotSetUpSignalHandler(String),
}

/// Initialize lazy pages:
/// 1) checks whether lazy pages is supported in current environment
/// 2) set signals handler
///
/// # Safety
/// See [`sys::setup_signal_handler`]
unsafe fn init_internal(version: LazyPagesVersion) -> Result<(), InitError> {
    use InitError::*;

    if LAZY_PAGES_ENABLED.with(|x| *x.borrow()) {
        log::trace!("Lazy-pages has been already enabled for current thread");
        return Ok(());
    }

    if LAZY_PAGES_CONTEXT.with(|ctx| *ctx.borrow() != LazyPagesExecutionContext::default()) {
        return Err(InitialContextIsNotDefault);
    }

    let ps = region::page::size();
    if ps > PAGE_STORAGE_GRANULARITY
        || WasmPageNumber::size() % ps != 0
        || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
        || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
    {
        return Err(NativePageSizeIsNotSuitable(ps));
    }

    if let Err(err) = sys::setup_signal_handler() {
        return Err(CanNotSetUpSignalHandler(err.to_string()));
    }

    // set impl version
    LAZY_PAGES_VERSION.with(|v| *v.borrow_mut() = version);

    log::debug!("Successfully enables lazy pages for current thread");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    Ok(())
}

/// Initialize lazy pages for current thread.
pub fn init(version: LazyPagesVersion) -> bool {
    if let Err(err) = unsafe { init_internal(version) } {
        log::debug!("Cannot initialize lazy pages: {}", err);
        false
    } else {
        true
    }
}
