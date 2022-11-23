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

use gear_core::{
    lazy_pages::{GlobalsCtx, Status},
    memory::{PageNumber, WasmPageNumber, PAGE_STORAGE_GRANULARITY},
};
use once_cell::sync::OnceCell;
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeSet, convert::TryFrom};

mod sys;
use sys::mprotect::{self, MprotectError};
pub use sys::{DefaultUserSignalHandler, ExceptionInfo, UserSignalHandler};

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
    /// Found a write signal from same page twice - restricted, see more in a head comment.
    #[display(fmt = "Any page cannot be released twice: {_0:?}")]
    DoubleRelease(PageNumber),
    #[display(fmt = "Protection error: {_0}")]
    #[from]
    MemoryProtection(region::Error),
    #[display(fmt = "Given instance host pointer is invalid")]
    HostInstancePointerIsInvalid,
    #[display(fmt = "Given pointer to globals access provider dyn object is invalid")]
    DynGlobalsAccessPointerIsInvalid,
    #[display(fmt = "Cannot charge gas from gas limit global")]
    CannotChargeGas,
    #[display(fmt = "Cannot charge gas from gas allowance global")]
    CannotChargeGasAllowance,
    #[display(fmt = "Status must be set before program execution")]
    StatusIsNone,
    #[display(fmt = "It's unknown wether memory access is read or write")]
    ReadOrWriteIsUnknown,
    #[display(
        fmt = "Second access cannot be read, because read protection must be removed for page"
    )]
    SecondAccessIsNotWrite,
}

pub(crate) type WasmAddr = u32;

#[derive(Clone, Copy)]
pub enum LazyPagesVersion {
    Version1,
}

#[cfg(test)]
mod tests;

#[derive(Default, Debug)]
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
    /// Context to access globals and works with them: charge gas, set status global.
    pub globals_ctx: Option<GlobalsCtx>,
    /// Lazy-pages status: indicates in which mod lazy-pages works actually.
    pub status: Option<Status>,
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

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum InitializeForProgramError {
    #[display(
        fmt = "WASM memory native address {:#x} is not aligned to the native page size",
        _0
    )]
    WasmMemAddrIsNotAligned(usize),
    #[display(fmt = "WASM memory size {_0:?} is bigger than u32::MAX bytes")]
    WasmMemSizeBiggerThenU32Max(WasmPageNumber),
    #[display(fmt = "WASM stack end addr {_0:?} > wasm mem size {_1:?}")]
    StackEndAddrBiggerThenSize(WasmPageNumber, WasmPageNumber),
    #[display(fmt = "{}", _0)]
    #[from]
    Mprotect(MprotectError),
    #[display(fmt = "Wasm memory end addr is out of usize: begin addr = {_0:#x}, size = {_1:#x}")]
    WasmMemoryEndAddrOverflow(usize, u32),
}

pub fn initialize_for_program(
    wasm_mem_addr: Option<usize>,
    wasm_mem_size: WasmPageNumber,
    stack_end_page: Option<WasmPageNumber>,
    program_prefix: Vec<u8>,
    globals_ctx: Option<GlobalsCtx>,
) -> Result<(), InitializeForProgramError> {
    use InitializeForProgramError::*;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.accessed_pages_addrs.clear();
        ctx.released_pages.clear();
        ctx.status = Some(Status::Normal);

        if let Some(addr) = wasm_mem_addr {
            if addr % region::page::size() != 0 {
                return Err(WasmMemAddrIsNotAligned(addr));
            }
        }
        ctx.wasm_mem_addr = wasm_mem_addr;

        let size_in_bytes = u32::try_from(wasm_mem_size.offset())
            .map_err(|_| WasmMemSizeBiggerThenU32Max(wasm_mem_size))?;
        if let Some(addr) = wasm_mem_addr {
            if addr.checked_add(size_in_bytes as usize).is_none() {
                return Err(WasmMemoryEndAddrOverflow(addr, size_in_bytes));
            }
        }
        ctx.wasm_mem_size = Some(size_in_bytes);

        ctx.stack_end_wasm_addr = if let Some(page) = stack_end_page {
            if page > wasm_mem_size {
                return Err(StackEndAddrBiggerThenSize(page, wasm_mem_size));
            }
            // `as u32` is safe, because page size is less then mem size
            page.offset() as u32
        } else {
            0
        };

        ctx.program_storage_prefix = Some(program_prefix);

        ctx.globals_ctx = globals_ctx;

        // Set protection if wasm memory exist.
        if let Some(addr) = wasm_mem_addr {
            let stack_end = stack_end_page.map(|p| p.offset()).unwrap_or(0);
            // `+` and `-` are safe because we checked, that `stack_end` is less than `wasm_mem_size`
            // and wasm end addr fits usize.
            let addr = addr + stack_end;
            let size = size_in_bytes as usize - stack_end;
            if size != 0 {
                mprotect::mprotect_interval(addr, size, false, false)?;
            }
        }

        log::trace!("Initialize lazy-pages for current program: {:?}", ctx);

        Ok(())
    })
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum MemoryProtectionError {
    #[from]
    #[display(fmt = "{_0}")]
    Mprotect(MprotectError),
    #[display(fmt = {"Wasm mem addr is not set before pages protect/unprotect"})]
    WasmMemAddrIsNotSet,
    #[display(fmt = {"Wasm mem size is not set before pages protect/unprotect"})]
    WasmMemSizeIsNotSet,
}

/// Protect lazy pages, after they had been unprotected.
pub fn set_lazy_pages_protection() -> Result<(), MemoryProtectionError> {
    use MemoryProtectionError::*;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let mem_addr = ctx.wasm_mem_addr.ok_or(WasmMemAddrIsNotSet)?;
        let start_offset = ctx.stack_end_wasm_addr as usize;
        let mem_size = ctx.wasm_mem_size.ok_or(WasmMemSizeIsNotSet)? as usize;

        // Set r/w protection for all pages except stack pages and released pages.
        let except_pages = ctx.released_pages.iter().copied();
        mprotect::mprotect_mem_interval_except_pages(
            mem_addr,
            start_offset,
            mem_size,
            except_pages,
            false,
            false,
        )?;

        // Set only write protection for already accessed, but not released pages.
        // `as u32` is safe because page size is less then `PAGE_STORAGE_GRANULARITY`.
        let lazy_page_size = region::page::size().max(PageNumber::size()) as u32;
        let read_only_pages = ctx
            .accessed_pages_addrs
            .iter()
            .filter(|&&addr| {
                // Checks whether first gear page in lazy page is not in released.
                let gear_page = PageNumber::new_from_addr(addr as usize);
                !ctx.released_pages.contains(&gear_page)
            })
            .map(|&addr| addr / lazy_page_size);
        mprotect::mprotect_pages(mem_addr, read_only_pages, lazy_page_size, true, false)?;

        // After that protections are:
        // 1) Only execution protection for stack pages.
        // 2) Only execution protection for released pages.
        // 3) Read and execution protection for accessed, but not released pages.
        // 4) r/w/e protections for all other WASM memory.

        Ok(())
    })
}

/// Unset lazy pages read/write protections.
pub fn unset_lazy_pages_protection() -> Result<(), MemoryProtectionError> {
    use MemoryProtectionError::*;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let addr = ctx.wasm_mem_addr.ok_or(WasmMemAddrIsNotSet)?;
        let size = ctx.wasm_mem_size.ok_or(WasmMemSizeIsNotSet)? as usize;
        mprotect::mprotect_interval(addr, size, true, true)?;
        Ok(())
    })
}

#[derive(derive_more::Display)]
#[display(fmt = "Wasm mem addr {_0:#x} is not aligned by native page")]
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
#[display(fmt = "Wasm mem size {_0:?} is bigger then u32::MAX bytes")]
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

pub fn get_status() -> Option<Status> {
    LAZY_PAGES_CONTEXT.with(|ctx| ctx.borrow().status)
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum InitError {
    #[display(fmt = "Native page size {_0:#x} is not suitable for lazy-pages")]
    NativePageSizeIsNotSuitable(usize),
    #[display(fmt = "Can not set signal handler: {_0}")]
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

    #[cfg(target_vendor = "apple")]
    {
        // Support debugging under lldb on Darwin.
        // When SIGBUS appears lldb will stuck on it forever, without this code.
        // See also: https://github.com/mono/mono/commit/8e75f5a28e6537e56ad70bf870b86e22539c2fb7.

        use mach::{
            exception_types::*, kern_return::*, mach_types::*, port::*, thread_status::*, traps::*,
        };

        extern "C" {
            // See https://web.mit.edu/darwin/src/modules/xnu/osfmk/man/task_set_exception_ports.html
            fn task_set_exception_ports(
                task: task_t,
                exception_mask: exception_mask_t,
                new_port: mach_port_t,
                behavior: exception_behavior_t,
                new_flavor: thread_state_flavor_t,
            ) -> kern_return_t;
        }

        #[cfg(target_arch = "x86_64")]
        static MACHINE_THREAD_STATE: i32 = x86_THREAD_STATE64 as i32;

        // Took const value from https://opensource.apple.com/source/cctools/cctools-870/include/mach/arm/thread_status.h
        // ```
        // #define ARM_THREAD_STATE64		6
        // ```
        #[cfg(target_arch = "aarch64")]
        static MACHINE_THREAD_STATE: i32 = 6;

        task_set_exception_ports(
            mach_task_self(),
            EXC_MASK_BAD_ACCESS,
            MACH_PORT_NULL,
            EXCEPTION_STATE_IDENTITY as exception_behavior_t,
            MACHINE_THREAD_STATE,
        );
    }

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
