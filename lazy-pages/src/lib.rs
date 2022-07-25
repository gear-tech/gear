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
//! Restrict any page handling in signal handler more then one time.
//! If some page will be released twice it means, that this page has been added
//! to lazy pages more then one time during current execution.
//! This situation may cause problems with memory data update in storage.
//! For example: one page has no data in storage, but allocated for current program.
//! Let's make some action for it:
//! 1) Change data in page: Default data  ->  Data1
//! 2) Free page
//! 3) Alloc page, data will Data2 (may be equal Data1).
//! 4) After alloc we can set page as lazy, to identify wether page is changed after allocation.
//! This means that we can skip page update in storage in case it wasnt changed after allocation.
//! 5) Write some data in page but do not change it Data2 -> Data2.
//! During this step signal handler writes Data2 as data for released page.
//! 6) After execution we will have Data2 in page. And Data2 in released. So, nothing will be updated
//! in storage. But program may have some significant data for next execution - so we have a bug.
//! To avoid this we restrict double releasing.
//! You can also check another cases in test: memory_access_cases.

// TODO: remove all deprecated code before release (issue #1147)

#![allow(useless_deprecated, deprecated)]

use gear_core::memory::{HostPointer, PageBuf, PageNumber, WasmPageNumber};
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeMap};

mod sys;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(
        fmt = "Exception is from unknown memory (WASM {:#x} > native page {:x})",
        wasm_mem_begin,
        native_page
    )]
    SignalFromUnknownMemory {
        wasm_mem_begin: usize,
        native_page: usize,
    },
    #[display(
        fmt = "Page data must contain {} bytes, actually has {}",
        expected,
        actual
    )]
    InvalidPageSize { expected: usize, actual: u32 },
    #[display(fmt = "Exception is from unknown memory: addr = {:?}, {:?}", _0, _1)]
    LazyPageNotExistForSignalAddr(*const (), PageNumber),
    /// Found a signal from same page twice - see more in head comment.
    #[display(fmt = "Page cannot be release page twice: {:?}", _0)]
    DoubleRelease(PageNumber),
    #[display(fmt = "Protection error: {}", _0)]
    #[from]
    MemoryProtection(region::Error),
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct LazyPagesExecutionContext {
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: Option<usize>,
    /// Current program prefix in storage
    pub program_storage_prefix: Option<Vec<u8>>,
    /// Page data, which has been in storage before current execution.
    /// For each lazy page, which has been accessed.
    pub released_lazy_pages: BTreeMap<PageNumber, Option<PageBuf>>,

    #[deprecated]
    /// Keys in storage for each lazy page.
    pub lazy_pages_info: BTreeMap<PageNumber, Vec<u8>>,
}

thread_local! {
    // NOTE: here we suppose, that each contract is executed in separate thread.
    // Or may be in one thread but consequentially.

    /// Identify whether signal handler is set for current thread
    static LAZY_PAGES_ENABLED: RefCell<bool> = RefCell::new(false);
    /// Lazy pages context for current execution
    static LAZY_PAGES_CONTEXT: RefCell<LazyPagesExecutionContext> = RefCell::new(Default::default());
}

#[deprecated]
pub fn get_lazy_pages_numbers() -> Vec<PageNumber> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().lazy_pages_info.keys().copied().collect())
}

/// Returns current wasm mem buffer pointer, if it's set
pub fn get_wasm_mem_addr() -> Option<HostPointer> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_addr)
}

/// Returns current wasm mem buffer size, if it's set
pub fn get_wasm_mem_size() -> Option<usize> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_size)
}

/// Set current wasm memory begin addr in global context
pub fn set_wasm_mem_begin_addr(wasm_mem_addr: HostPointer) {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_addr.insert(wasm_mem_addr);
    });
}

/// Set current wasm memory size in global context
pub fn set_wasm_mem_size(wasm_mem_size: usize) {
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
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().released_lazy_pages.keys().copied().collect())
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
        let page_end = page.offset() + PageNumber::size();
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
    let end_offset = (max_page + 1) as usize * PageNumber::size();
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
pub fn get_released_page_data(page: PageNumber) -> Option<PageBuf> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow_mut().released_lazy_pages.get_mut(&page)?.take())
}

/// Initialize lazy pages:
/// 1) checks whether lazy pages is supported in current environment
/// 2) set signals handler
///
/// # Safety
/// See [`sys::setup_signal_handler`]
pub unsafe fn init() -> bool {
    if LAZY_PAGES_ENABLED.with(|x| *x.borrow()) {
        log::trace!("Lazy-pages has been already enabled");
        return true;
    }

    if LAZY_PAGES_CONTEXT.with(|ctx| *ctx.borrow() != LazyPagesExecutionContext::default()) {
        log::error!("Lazy pages context has not default values before lazy pages initialization");
        return false;
    }

    let ps = region::page::size();
    if ps > WasmPageNumber::size()
        || WasmPageNumber::size() % ps != 0
        || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
        || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
    {
        log::error!("Unsupported native pages size: {:#x}", ps);
        return false;
    }

    if let Err(err) = sys::setup_signal_handler() {
        log::error!("Failed to setup kernel signal handler: {}", err);
        return false;
    }

    log::debug!("Lazy pages are successfully enabled");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    true
}
