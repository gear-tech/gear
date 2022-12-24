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

use gear_core::memory::{
    PageNumber, PageU32Size, WasmPageNumber, GEAR_PAGE_SIZE, PAGE_STORAGE_GRANULARITY,
};
use once_cell::sync::OnceCell;
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeSet, num::NonZeroU32};

mod sys;
use sys::{DefaultUserSignalHandler, UserSignalHandler};

mod mprotect;
use mprotect::MprotectError;

mod utils;

#[cfg(test)]
mod tests;

/// Initialize lazy-pages once for process.
static LAZY_PAGES_INITIALIZED: OnceCell<Result<(), InitError>> = OnceCell::new();

#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) enum Error {
    #[display(fmt = "Accessed memory interval is out of wasm memory")]
    OutOfWasmMemoryAccess,
    #[display(fmt = "Signals cannot come from WASM program virtual stack memory")]
    SignalFromStackMemory,
    #[display(fmt = "Signals cannot come from released page")]
    SignalFromReleasedPage,
    #[display(fmt = "Read access signal cannot come from already accessed page")]
    ReadAccessSignalFromAccessedPage,
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "WASM memory size is not set")]
    WasmMemSizeIsNotSet,
    #[display(fmt = "Program pages prefix in storage is not set")]
    ProgramPrefixIsNotSet,
    #[display(fmt = "Page data in storage must contain {expected} bytes, actually has {actual}")]
    InvalidPageDataSize { expected: u32, actual: u32 },
    #[display(fmt = "Any page cannot be released twice: {_0:?}")]
    DoubleRelease(LazyPage),
    #[display(fmt = "Protection error: {_0}")]
    #[from]
    MemoryProtection(region::Error),
    #[display(fmt = "It's unknown wether memory access is read or write")]
    ReadOrWriteIsUnknown,
}

#[derive(Clone, Copy)]
pub enum LazyPagesVersion {
    Version1,
}

#[derive(Default, Debug)]
pub(crate) struct LazyPagesExecutionContext {
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<usize>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: Option<WasmPageNumber>,
    /// Current program prefix in storage
    pub program_storage_prefix: Option<Vec<u8>>,
    /// Wasm addresses of lazy-pages, that have been read or write accessed at least once.
    /// Lazy page here is page, which has `size = max(native_page_size, gear_page_size)`.
    pub accessed_pages: BTreeSet<LazyPage>,
    /// End of stack wasm address. Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy-pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `released_pages`,
    /// which means that they will never be updated in storage.
    pub stack_end_wasm_page: WasmPageNumber,
    /// Gear pages, which has been write accessed.
    pub released_pages: BTreeSet<LazyPage>,
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
    #[display(fmt = "WASM memory native address {_0:#x} is not aligned to the native page size")]
    WasmMemAddrIsNotAligned(usize),
    #[display(fmt = "WASM memory size {_0:?} is bigger than u32::MAX bytes")]
    WasmMemSizeBiggerThenU32Max(WasmPageNumber),
    #[display(fmt = "WASM stack end addr {_0:?} > wasm mem size {_1:?}")]
    StackEndAddrBiggerThenSize(WasmPageNumber, WasmPageNumber),
    #[display(fmt = "{_0}")]
    #[from]
    Mprotect(MprotectError),
    #[display(fmt = "Wasm memory end addr is out of usize: begin addr = {_0:#x}, size = {_1:#x}")]
    WasmMemoryEndAddrOverflow(usize, u32),
    #[display(fmt = "Prefix of storage with memory pages was not set")]
    MemoryPagesPrefixNotSet,
}

pub fn initialize_for_program(
    wasm_mem_addr: Option<usize>,
    wasm_mem_size: WasmPageNumber,
    stack_end_page: Option<WasmPageNumber>,
    program_id: [u8; 32],
) -> Result<(), InitializeForProgramError> {
    use InitializeForProgramError::*;
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();

        let prefix = match ctx.program_storage_prefix.take() {
            Some(mut prefix) => {
                prefix.extend_from_slice(&program_id);

                prefix
            }
            None => return Err(MemoryPagesPrefixNotSet),
        };

        *ctx = LazyPagesExecutionContext::default();

        ctx.program_storage_prefix = Some(prefix);

        if let Some(addr) = wasm_mem_addr {
            if addr % region::page::size() != 0 {
                return Err(WasmMemAddrIsNotAligned(addr));
            }
        }
        ctx.wasm_mem_addr = wasm_mem_addr;

        if let Some(addr) = wasm_mem_addr {
            if addr.checked_add(wasm_mem_size.offset() as usize).is_none() {
                return Err(WasmMemoryEndAddrOverflow(addr, wasm_mem_size.offset()));
            }
        }
        ctx.wasm_mem_size = Some(wasm_mem_size);

        ctx.stack_end_wasm_page = if let Some(stack_end_page) = stack_end_page {
            if stack_end_page > wasm_mem_size {
                return Err(StackEndAddrBiggerThenSize(stack_end_page, wasm_mem_size));
            }
            stack_end_page
        } else {
            WasmPageNumber::zero()
        };

        // Set protection if wasm memory exist.
        if let Some(addr) = wasm_mem_addr {
            // `+` and `-` are safe because we checked, that `stack_end` is less than `wasm_mem_size`
            // and wasm end addr fits usize.
            let addr = addr + ctx.stack_end_wasm_page.offset() as usize;
            let size = wasm_mem_size.offset() - ctx.stack_end_wasm_page.offset();
            if size != 0 {
                mprotect::mprotect_interval(addr, size as usize, false, false)?;
            }
        }

        log::trace!("Initialize lazy-pages for current program: {:?}", ctx);

        Ok(())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
struct LazyPage(u32);

impl PageU32Size for LazyPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(GEAR_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(region::page::size().max(GEAR_PAGE_SIZE) as u32) }
    }

    fn raw(&self) -> u32 {
        self.0
    }

    unsafe fn new_unchecked(num: u32) -> Self {
        Self(num)
    }
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
        let start_offset = ctx.stack_end_wasm_page.offset();
        let mem_size = ctx.wasm_mem_size.ok_or(WasmMemSizeIsNotSet)?.offset();

        // Set r/w protection for all pages except stack pages and released pages.
        mprotect::mprotect_mem_interval_except_pages(
            mem_addr,
            start_offset as usize,
            mem_size as usize,
            ctx.released_pages.iter().copied(),
            false,
            false,
        )?;

        // Set only write protection for already accessed, but not released pages.
        let read_only_pages = ctx
            .accessed_pages
            .iter()
            .filter(|&&page| !ctx.released_pages.contains(&page))
            .copied();
        mprotect::mprotect_pages(mem_addr, read_only_pages, true, false)?;

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
        let size = ctx.wasm_mem_size.ok_or(WasmMemSizeIsNotSet)?.offset();
        mprotect::mprotect_interval(addr, size as usize, true, true)?;
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
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_size.insert(size);
    });
    Ok(())
}

/// Returns vec of lazy-pages which has been accessed
pub fn get_released_pages() -> Vec<PageNumber> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow()
            .released_pages
            .iter()
            .flat_map(|page| page.to_pages_iter())
            .collect()
    })
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
            let gear_ps = PageNumber::size() as usize;
            if ps > PAGE_STORAGE_GRANULARITY
                || PAGE_STORAGE_GRANULARITY % ps != 0
                || (ps > gear_ps && ps % gear_ps != 0)
                || (ps < gear_ps && gear_ps % ps != 0)
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
pub(crate) fn init_with_handler<H: UserSignalHandler>(
    version: LazyPagesVersion,
    pages_final_prefix: [u8; 32],
) -> bool {
    // Set version even if it has been already set, because it can be changed after runtime upgrade.
    LAZY_PAGES_VERSION.with(|v| *v.borrow_mut() = version);
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        *ctx = LazyPagesExecutionContext::default();

        let mut prefix = Vec::with_capacity(64);
        prefix.extend_from_slice(&pages_final_prefix);
        ctx.program_storage_prefix = Some(prefix);
    });

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

pub fn init(version: LazyPagesVersion, pages_final_prefix: [u8; 32]) -> bool {
    init_with_handler::<DefaultUserSignalHandler>(version, pages_final_prefix)
}
