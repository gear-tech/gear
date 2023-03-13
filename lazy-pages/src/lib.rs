// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
//! During execution data from storage is loaded for all pages, which has been accessed
//! and which has data in storage.
//! See also `process::process_lazy_pages`, `signal`, `host_func` for more information.
//!
//! Note: currently we restrict twice write signal from same page during one execution.
//! It's not necessary behavior, but more simple and safe.

use common::LazyPagesExecutionContext;
use gear_backend_common::lazy_pages::{GlobalsConfig, LazyPagesWeights, Status};
use gear_core::memory::{
    GearPage, PageU32Size, WasmPage, GEAR_PAGE_SIZE, PAGE_STORAGE_GRANULARITY,
};
use sp_std::vec::Vec;
use std::cell::RefCell;

mod common;
mod globals;
mod host_func;
mod mprotect;
mod process;
mod signal;
mod sys;
mod init_flag;
mod utils;

use crate::init_flag::InitializationFlag;

#[cfg(test)]
mod tests;

pub use common::LazyPagesVersion;
pub use host_func::pre_process_memory_accesses;

use mprotect::MprotectError;
use signal::{DefaultUserSignalHandler, UserSignalHandler};

// These constants are used both in runtime and in lazy-pages backend,
// so we make here additional checks. If somebody would change these values
// in runtime, then he also should pay attention to support new values here:
// 1) must rebuild node after that.
// 2) must support old runtimes: need to make lazy-pages version with old constants values.
static_assertions::const_assert_eq!(GEAR_PAGE_SIZE, 0x1000);
static_assertions::const_assert_eq!(PAGE_STORAGE_GRANULARITY, 0x4000);

/// Initialize lazy-pages once for process.
static LAZY_PAGES_INITIALIZED: InitializationFlag = InitializationFlag::new();

thread_local! {
    // NOTE: here we suppose, that each contract is executed in separate thread.
    // Or may be in one thread but consequentially.

    /// Lazy-pages impl version. Different runtimes may require different impl of lazy-pages functionality.
    /// NOTE: be dangerous when use it and pay attention process and thread initialization.
    static LAZY_PAGES_RUNTIME_CONTEXT: RefCell<(LazyPagesVersion, Vec<u8>)> = RefCell::new((LazyPagesVersion::Version1, vec![]));
    /// Lazy-pages context for current execution.
    static LAZY_PAGES_PROGRAM_CONTEXT: RefCell<LazyPagesExecutionContext> = RefCell::new(Default::default());
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum InitializeForProgramError {
    #[display(fmt = "WASM memory native address {_0:#x} is not aligned to the native page size")]
    WasmMemAddrIsNotAligned(usize),
    #[display(fmt = "WASM memory size {_0:?} is bigger than u32::MAX bytes")]
    WasmMemSizeBiggerThenU32Max(WasmPage),
    #[display(fmt = "WASM stack end addr {_0:?} > wasm mem size {_1:?}")]
    StackEndAddrBiggerThenSize(WasmPage, WasmPage),
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
    wasm_mem_size: WasmPage,
    stack_end: Option<WasmPage>,
    program_id: Vec<u8>,
    globals_config: Option<GlobalsConfig>,
    lazy_pages_weights: LazyPagesWeights,
) -> Result<(), InitializeForProgramError> {
    use InitializeForProgramError::*;

    let mut program_storage_prefix = LAZY_PAGES_RUNTIME_CONTEXT.with(|context| {
        let (_, ref prefix) = *context.borrow();
        prefix.clone()
    });

    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        *ctx = LazyPagesExecutionContext::default();

        ctx.lazy_pages_weights = lazy_pages_weights;

        program_storage_prefix.extend_from_slice(&program_id);
        ctx.set_program_prefix(program_storage_prefix);
        ctx.status.replace(Status::Normal);

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

        ctx.stack_end_wasm_page = if let Some(stack_end) = stack_end {
            if stack_end > wasm_mem_size {
                return Err(StackEndAddrBiggerThenSize(stack_end, wasm_mem_size));
            }
            stack_end
        } else {
            WasmPage::zero()
        };

        ctx.globals_config = globals_config;

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
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
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
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
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

    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_addr.insert(wasm_mem_addr);
    });

    Ok(())
}

#[derive(derive_more::Display)]
#[display(fmt = "Wasm mem size {_0:?} is bigger then u32::MAX bytes")]
pub struct WasmMemSizeError(WasmPage);

/// Set current wasm memory size in global context
pub fn set_wasm_mem_size(size: WasmPage) -> Result<(), WasmMemSizeError> {
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
        let _ = ctx.borrow_mut().wasm_mem_size.insert(size);
    });
    Ok(())
}

/// Returns vec of lazy-pages which has been accessed
pub fn get_released_pages() -> Vec<GearPage> {
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| {
        ctx.borrow()
            .released_pages
            .iter()
            .flat_map(|page| page.to_pages_iter())
            .collect()
    })
}

pub fn get_status() -> Option<Status> {
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| ctx.borrow().status)
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

    LAZY_PAGES_INITIALIZED.get_or_init(|| {
        let ps = region::page::size();
        let gear_ps = GearPage::size() as usize;
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
}

#[cfg(test)]
pub(crate) fn reset_init_flag() {
    LAZY_PAGES_INITIALIZED.reset();
}

/// Initialize lazy-pages for current thread.
fn init_with_handler<H: UserSignalHandler>(
    version: LazyPagesVersion,
    pages_final_prefix: Vec<u8>,
) -> bool {
    // Set version even if it has been already set, because it can be changed after runtime upgrade.
    LAZY_PAGES_RUNTIME_CONTEXT.with(|v| {
        *v.borrow_mut() = (version, pages_final_prefix);
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

pub fn init(version: LazyPagesVersion, pages_final_prefix: Vec<u8>) -> bool {
    init_with_handler::<DefaultUserSignalHandler>(version, pages_final_prefix)
}
