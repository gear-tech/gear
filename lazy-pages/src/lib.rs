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

//! Lazy-pages support.
//! In runtime data for contract wasm memory pages can be loaded in lazy manner.
//! All pages, which is supposed to be lazy, must be mprotected before contract execution.
//! During execution data from storage is loaded for all pages, which has been accessed.
//! See also `sys::user_signal_handler` in the source code.
//!
//! Currently we restrict twice write signal from same page during one execution.
//! It's not necessary behavior, but more simple and safe.

use gear_core::memory::{PageNumber, WasmPageNumber, PAGE_STORAGE_GRANULARITY};
use once_cell::sync::OnceCell;
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeSet, convert::TryFrom};

mod sys;
pub use crate::sys::{DefaultUserSignalHandler, ExceptionInfo, UserSignalHandler};

/// Initialize lazy-pages once for process.
static LAZY_PAGES_INITIALIZED: OnceCell<Result<(), InitError>> = OnceCell::new();

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "WASM memory size is not set")]
    WasmMemSizeIsNotSet,
    #[display(fmt = "Overflow of address arithmetic operation")]
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
        fmt = "Signal addr {:#x} is from WASM program virtual stack memory [0, {:#x})",
        wasm_addr,
        stack_end
    )]
    SignalFromStackMemory {
        wasm_addr: WasmAddr,
        stack_end: WasmAddr,
    },
    #[display(
        fmt = "Accessed pages do not lay in WASM memory: [{:#x}, {:#x}) not in [{:#x}, {:#x})",
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
}

pub(crate) type WasmAddr = u32;

#[derive(Clone, Copy)]
pub enum LazyPagesVersion {
    Version1,
}

#[derive(Default, PartialEq, Eq, Debug)]
pub(crate) struct LazyPagesExecutionContext {
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<usize>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: Option<u32>,
    /// Current program prefix in storage
    pub program_storage_prefix: Option<Vec<u8>>,
    /// Wasm addresses of lazy-pages, that have been read or write accessed at least once.
    /// Lazy page here is page, which has `size = max(native_page_size, gear_page_size)`.
    pub accessed_pages_addrs: BTreeSet<WasmAddr>,
    /// End of stack wasm address. Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy-pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `released_pages`,
    /// which means that they will never be updated in storage.
    pub stack_end_wasm_addr: WasmAddr,
    /// Gear pages, which has been write accessed.
    pub released_pages: BTreeSet<PageNumber>,
}

thread_local! {
    // NOTE: here we suppose, that each contract is executed in separate thread.
    // Or may be in one thread but consequentially.

    /// Lazy-pages impl version. Different runtimes may require different impl of lazy-pages functionality.
    /// NOTE: be dangerous when use it and pay attention process and thread initialization.
    static LAZY_PAGES_VERSION: RefCell<LazyPagesVersion> = RefCell::new(LazyPagesVersion::Version1);
    /// Lazy-pages context for current execution.
    static LAZY_PAGES_CONTEXT: RefCell<LazyPagesExecutionContext> = RefCell::new(Default::default());
}

#[derive(Debug, derive_more::Display)]
pub enum InitializeForProgramError {
    #[display(
        fmt = "WASM memory native address {:#x} is not aligned to the native page size",
        _0
    )]
    WasmMemAddrIsNotAligned(usize),
    #[display(fmt = "WASM memory size {:?} is bigger than u32::MAX bytes", _0)]
    WasmMemSizeBiggerThenU32Max(WasmPageNumber),
    #[display(fmt = "WASM stack end addr {:?} > wasm mem size {:?}", _0, _1)]
    StackEndAddrBiggerThenSize(WasmPageNumber, WasmPageNumber),
}

pub fn initialize_for_program(
    wasm_mem_addr: Option<usize>,
    wasm_mem_size: WasmPageNumber,
    stack_end_page: Option<WasmPageNumber>,
    program_prefix: Vec<u8>,
) -> Result<(), InitializeForProgramError> {
    use InitializeForProgramError::*;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.accessed_pages_addrs.clear();
        ctx.released_pages.clear();

        ctx.wasm_mem_addr = if let Some(addr) = wasm_mem_addr {
            if addr % region::page::size() != 0 {
                return Err(WasmMemAddrIsNotAligned(addr));
            }
            Some(addr)
        } else {
            None
        };

        let size_in_bytes = u32::try_from(wasm_mem_size.offset())
            .map_err(|_| WasmMemSizeBiggerThenU32Max(wasm_mem_size))?;
        ctx.wasm_mem_size = Some(size_in_bytes);

        ctx.stack_end_wasm_addr = if let Some(page) = stack_end_page {
            if page > wasm_mem_size {
                return Err(StackEndAddrBiggerThenSize(page, wasm_mem_size));
            }
            // `as u32` is safe, because page is less then mem size
            page.offset() as u32
        } else {
            0
        };

        ctx.program_storage_prefix = Some(program_prefix);

        log::trace!("Initialize lazy-pages for current program: {:?}", ctx);

        Ok(())
    })
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
pub fn get_wasm_mem_addr() -> Option<usize> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_addr)
}

/// Returns current wasm mem buffer size, if it's set
pub fn get_wasm_mem_size() -> Option<u32> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().wasm_mem_size)
}

#[derive(derive_more::Display)]
#[display(fmt = "Wasm mem addr {:#x} is not aligned by native page", _0)]
pub struct WasmMemAddrError(usize);

/// Set current wasm memory begin addr in global context
pub fn set_wasm_mem_begin_addr(wasm_mem_addr: usize) -> Result<(), WasmMemAddrError> {
    if wasm_mem_addr % region::page::size() != 0 {
        return Err(WasmMemAddrError(wasm_mem_addr));
    }

    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_addr.insert(wasm_mem_addr);
    });

    Ok(())
}

#[derive(derive_more::Display)]
#[display(fmt = "Wasm mem size {:?} is bigger then u32::MAX bytes", _0)]
pub struct WasmMemSizeError(WasmPageNumber);

/// Set current wasm memory size in global context
pub fn set_wasm_mem_size(size: WasmPageNumber) -> Result<(), WasmMemSizeError> {
    let size_in_bytes = u32::try_from(size.offset()).map_err(|_| WasmMemSizeError(size))?;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_size.insert(size_in_bytes);
    });
    Ok(())
}

/// Returns vec of lazy-pages which has been accessed
pub fn get_released_pages() -> Vec<PageNumber> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().released_pages.iter().copied().collect())
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum InitError {
    #[display(fmt = "Native page size {:#x} is not suitable for lazy-pages", _0)]
    NativePageSizeIsNotSuitable(usize),
    #[display(fmt = "Can not set signal handler: {}", _0)]
    CanNotSetUpSignalHandler(String),
}

/// Initialize lazy-pages once for process:
/// 1) checks whether lazy-pages is supported in current environment
/// 2) set signals handler
///
/// # Safety
/// See [`sys::setup_signal_handler`]
unsafe fn init_for_process<H: UserSignalHandler>() -> Result<(), InitError> {
    use InitError::*;

    LAZY_PAGES_INITIALIZED
        .get_or_init(|| {
            let ps = region::page::size();
            if ps > PAGE_STORAGE_GRANULARITY
                || PAGE_STORAGE_GRANULARITY % ps != 0
                || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
                || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
            {
                return Err(NativePageSizeIsNotSuitable(ps));
            }

            if let Err(err) = sys::setup_signal_handler::<H>() {
                return Err(CanNotSetUpSignalHandler(err.to_string()));
            }

            log::trace!("Successfully initialize lazy-pages for process");

            Ok(())
        })
        .clone()
}

/// Initialize lazy-pages for current thread.
pub fn init<H: UserSignalHandler>(version: LazyPagesVersion) -> bool {
    // Set version even if it has been already set, because it can be changed after runtime upgrade.
    LAZY_PAGES_VERSION.with(|v| *v.borrow_mut() = version);

    if let Err(err) = unsafe { init_for_process::<H>() } {
        log::debug!("Cannot initialize lazy-pages for process: {}", err);
        return false;
    }

    if let Err(err) = unsafe { sys::init_for_thread() } {
        log::debug!("Cannot initialize lazy-pages for thread: {}", err);
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use crate::*;
    use region::Protection;
    use std::ptr;

    #[test]
    fn read_write_flag_works() {
        unsafe fn protect(access: bool) {
            let protection = if access {
                Protection::READ_WRITE
            } else {
                Protection::NONE
            };
            let page_size = region::page::size();
            let addr = MEM_ADDR;
            region::protect(addr, page_size, protection).unwrap();
        }

        unsafe fn invalid_write() {
            ptr::write_volatile(MEM_ADDR as *mut _, 123);
            protect(false);
        }

        unsafe fn invalid_read() {
            let _: u8 = ptr::read_volatile(MEM_ADDR);
            protect(false);
        }

        static mut COUNTER: u32 = 0;
        static mut MEM_ADDR: *const u8 = ptr::null_mut();

        struct TestHandler;

        impl UserSignalHandler for TestHandler {
            unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
                let write_expected = COUNTER % 2 == 0;
                assert_eq!(info.is_write, Some(write_expected));

                protect(true);

                COUNTER += 1;

                Ok(())
            }
        }

        assert!(init::<TestHandler>(LazyPagesVersion::Version1));

        let page_size = region::page::size();
        let addr = region::alloc(page_size, Protection::NONE).unwrap();

        unsafe {
            MEM_ADDR = addr.as_ptr();

            invalid_write();
            invalid_read();
            invalid_write();
            invalid_read();
        }
    }
}
