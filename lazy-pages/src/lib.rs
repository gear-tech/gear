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

use common::{LazyPagesExecutionContext, LazyPagesRuntimeContext};
use gear_backend_common::lazy_pages::{GlobalsAccessConfig, Status};
use pages::{PageNumber, WasmPageNumber};
use sp_std::vec::Vec;
use std::{cell::RefCell, convert::TryInto, num::NonZeroU32};

mod common;
mod globals;
mod host_func;
mod init_flag;
mod mprotect;
mod pages;
mod process;
mod signal;
mod sys;
mod utils;

use crate::{
    common::{
        ContextError, GlobalNames, LazyPagesContext, PagePrefix, PageSizes, WeightNo, Weights,
    },
    globals::{GlobalNo, GlobalsContext},
    init_flag::InitializationFlag,
    pages::{PageDynSize, PageSizeNo},
};

#[cfg(test)]
mod tests;

pub use common::LazyPagesVersion;
pub use host_func::pre_process_memory_accesses;

use mprotect::MprotectError;
use signal::{DefaultUserSignalHandler, UserSignalHandler};

/// Initialize lazy-pages once for process.
static LAZY_PAGES_INITIALIZED: InitializationFlag = InitializationFlag::new();

thread_local! {
    // NOTE: here we suppose, that each contract is executed in separate thread.
    // Or may be in one thread but consequentially.

    static LAZY_PAGES_CONTEXT: RefCell<LazyPagesContext> = RefCell::new(Default::default());
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "WASM memory native address {_0:#x} is not aligned to the native page size")]
    WasmMemAddrIsNotAligned(usize),
    #[display(fmt = "{_0}")]
    #[from]
    Mprotect(MprotectError),
    #[display(fmt = "Wasm memory end addr is out of usize: begin addr = {_0:#x}, size = {_1:#x}")]
    WasmMemoryEndAddrOverflow(usize, u32),
    #[display(fmt = "Prefix of storage with memory pages was not set")]
    MemoryPagesPrefixNotSet,
    #[display(fmt = "Memory size must be null when memory host addr is not set")]
    MemorySizeIsNotNull,
    #[display(fmt = "Wasm mem size is too big")]
    WasmMemSizeOverflow,
    #[display(fmt = "Stack end offset cannot be bigger than memory size")]
    StackEndBiggerThanMemSize,
    #[display(fmt = "Stack end offset is too big")]
    StackEndOverflow,
    #[display(fmt = "Wasm addr and size are not changed, so host func call is needless")]
    NothingToChange,
    #[display(fmt = "Wasm memory addr must be set, when trying to change something in lazy pages")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "{_0}")]
    #[from]
    GlobalContext(ContextError),
    #[display(fmt = "Wrong weights amount: get {_0}, must be {_1}")]
    WrongWeightsAmount(usize, usize),
}

fn check_memory_interval(addr: usize, size: u32) -> Result<(), Error> {
    addr.checked_add(size as usize)
        .ok_or(Error::WasmMemoryEndAddrOverflow(addr, size))
        .map(|_| ())
}

pub fn initialize_for_program(
    wasm_mem_addr: Option<usize>,
    wasm_mem_size: u32,
    stack_end: Option<u32>,
    program_id: Vec<u8>,
    globals_config: Option<GlobalsAccessConfig>,
    weights: Vec<u64>,
) -> Result<(), Error> {
    // Initialize new execution context
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let runtime_ctx = ctx.runtime_context_mut()?;

        // Check wasm program memory host address
        if let Some(addr) = wasm_mem_addr {
            if addr % region::page::size() != 0 {
                return Err(Error::WasmMemAddrIsNotAligned(addr));
            }
        }

        // Check stack_end is less or equal than wasm memory size
        let stack_end = stack_end.unwrap_or_default();
        if wasm_mem_size < stack_end {
            return Err(Error::StackEndBiggerThanMemSize);
        }

        let wasm_mem_size =
            WasmPageNumber::new(wasm_mem_size, runtime_ctx).ok_or(Error::WasmMemSizeOverflow)?;
        let wasm_mem_size_in_bytes = wasm_mem_size.offset(runtime_ctx);

        // Check wasm program memory size
        if let Some(addr) = wasm_mem_addr {
            check_memory_interval(addr, wasm_mem_size_in_bytes)?;
        } else if wasm_mem_size_in_bytes != 0 {
            return Err(Error::MemorySizeIsNotNull);
        }

        let stack_end =
            WasmPageNumber::new(stack_end, runtime_ctx).ok_or(Error::StackEndOverflow)?;

        let weights: Weights = weights.try_into().map_err(|ws: Vec<u64>| {
            Error::WrongWeightsAmount(ws.len(), WeightNo::Amount as usize)
        })?;

        let execution_ctx = LazyPagesExecutionContext {
            page_sizes: runtime_ctx.page_sizes,
            weights,
            wasm_mem_addr,
            wasm_mem_size,
            program_storage_prefix: PagePrefix::new_from_program_prefix(
                runtime_ctx
                    .pages_storage_prefix
                    .iter()
                    .chain(program_id.iter())
                    .copied()
                    .collect(),
            ),
            accessed_pages: Default::default(),
            write_accessed_pages: Default::default(),
            stack_end,
            globals_context: globals_config.map(|cfg| GlobalsContext {
                names: runtime_ctx.global_names.clone(),
                access_ptr: cfg.access_ptr,
                access_mod: cfg.access_mod,
            }),
            status: Status::Normal,
        };

        // Set protection if wasm memory exist.
        if let Some(addr) = wasm_mem_addr {
            let stack_end_offset = execution_ctx.stack_end.offset(&execution_ctx);
            log::trace!("{addr:#x} {stack_end_offset:#x}");
            // `+` and `-` are safe because we checked
            // that `stack_end` is less or equal to `wasm_mem_size` and wasm end addr fits usize.
            let addr = addr + stack_end_offset as usize;
            let size = wasm_mem_size_in_bytes - stack_end_offset;
            if size != 0 {
                mprotect::mprotect_interval(addr, size as usize, false, false)?;
            }
        }

        ctx.set_execution_context(execution_ctx);

        log::trace!("Initialize lazy-pages for current program: {:?}", ctx);

        Ok(())
    })
}

/// Protect lazy pages, after they had been unprotected.
pub fn set_lazy_pages_protection() -> Result<(), Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let ctx = ctx.execution_context()?;
        let mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
        let start_offset = ctx.stack_end.offset(ctx);
        let mem_size = ctx.wasm_mem_size.offset(ctx);

        // Set r/w protection for all pages except stack pages and write accessed pages.
        mprotect::mprotect_mem_interval_except_pages(
            mem_addr,
            start_offset as usize,
            mem_size as usize,
            ctx.write_accessed_pages.iter().copied(),
            ctx,
            false,
            false,
        )?;

        // Set only write protection for already accessed, but not write accessed pages.
        let read_only_pages = ctx
            .accessed_pages
            .iter()
            .filter(|&&page| !ctx.write_accessed_pages.contains(&page))
            .copied();
        mprotect::mprotect_pages(mem_addr, read_only_pages, ctx, true, false)?;

        // After that protections are:
        // 1) Only execution protection for stack pages.
        // 2) Only execution protection for write accessed pages.
        // 3) Read and execution protection for accessed, but not write accessed pages.
        // 4) r/w/e protections for all other WASM memory.

        Ok(())
    })
}

/// Unset lazy pages read/write protections.
pub fn unset_lazy_pages_protection() -> Result<(), Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let ctx = ctx.execution_context()?;
        let addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
        let size = ctx.wasm_mem_size.offset(ctx);
        mprotect::mprotect_interval(addr, size as usize, true, true)?;
        Ok(())
    })
}

/// Set current wasm memory begin addr in global context
pub fn change_wasm_mem_addr_and_size(addr: Option<usize>, size: Option<u32>) -> Result<(), Error> {
    if matches!((addr, size), (None, None)) {
        return Err(Error::NothingToChange);
    }

    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let ctx = ctx.execution_context_mut()?;

        let addr = match addr {
            Some(addr) => match addr % region::page::size() {
                0 => addr,
                _ => return Err(Error::WasmMemAddrIsNotAligned(addr)),
            },

            None => match ctx.wasm_mem_addr {
                Some(addr) => addr,
                None => return Err(Error::WasmMemAddrIsNotSet),
            },
        };

        let size = match size {
            Some(size) => WasmPageNumber::new(size, ctx).ok_or(Error::WasmMemSizeOverflow)?,
            None => ctx.wasm_mem_size,
        };

        check_memory_interval(addr, size.offset(ctx))?;

        ctx.wasm_mem_addr = Some(addr);
        ctx.wasm_mem_size = size;

        Ok(())
    })
}

/// Returns vec of lazy-pages which has been accessed
pub fn write_accessed_pages() -> Result<Vec<u32>, Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow()
            .execution_context()
            .map(|ctx| ctx.write_accessed_pages.iter().map(|p| p.raw()).collect())
            .map_err(Into::into)
    })
}

pub fn status() -> Result<Status, Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow()
            .execution_context()
            .map(|ctx| ctx.status)
            .map_err(Into::into)
    })
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum InitError {
    #[display(fmt = "Wrong page sizes amount: get {_0}, must be {_1}")]
    WrongSizesAmount(usize, usize),
    #[display(fmt = "Wrong global names amount: get {_0}, must be {_1}")]
    WrongGlobalNamesAmount(usize, usize),
    #[display(fmt = "Not suitable page sizes")]
    NotSuitablePageSizes,
    #[display(fmt = "Can not set signal handler: {_0}")]
    CanNotSetUpSignalHandler(String),
    #[display(fmt = "Failed to init for thread: {_0}")]
    InitForThread(String),
    #[display(fmt = "Provided by runtime memory page size cannot be zero")]
    ZeroPageSize,
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
    _version: LazyPagesVersion,
    page_sizes: Vec<u32>,
    global_names: Vec<String>,
    pages_storage_prefix: Vec<u8>,
) -> Result<(), InitError> {
    use InitError::*;

    // Check that sizes are not zero
    let page_sizes = page_sizes
        .into_iter()
        .map(TryInto::<NonZeroU32>::try_into)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ZeroPageSize)?;

    let page_sizes: PageSizes = match page_sizes.try_into() {
        Ok(sizes) => sizes,
        Err(sizes) => return Err(WrongSizesAmount(sizes.len(), PageSizeNo::Amount as usize)),
    };

    // Check sizes suitability
    let wasm_page_size = page_sizes[PageSizeNo::WasmSizeNo as usize];
    let gear_page_size = page_sizes[PageSizeNo::GearSizeNo as usize];
    let native_page_size = region::page::size();
    if wasm_page_size < gear_page_size
        || (gear_page_size.get() as usize) < native_page_size
        || !u32::is_power_of_two(wasm_page_size.get())
        || !u32::is_power_of_two(gear_page_size.get())
        || !usize::is_power_of_two(native_page_size)
    {
        return Err(NotSuitablePageSizes);
    }

    let global_names: GlobalNames = match global_names.try_into() {
        Ok(names) => names,
        Err(names) => {
            return Err(WrongGlobalNamesAmount(
                names.len(),
                GlobalNo::Amount as usize,
            ))
        }
    };

    // Set version even if it has been already set, because it can be changed after runtime upgrade.
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow_mut()
            .set_runtime_context(LazyPagesRuntimeContext {
                page_sizes,
                global_names,
                pages_storage_prefix,
            })
    });

    unsafe { init_for_process::<H>()? }

    unsafe { sys::init_for_thread().map_err(InitForThread)? }

    Ok(())
}

pub fn init(
    version: LazyPagesVersion,
    page_sizes: Vec<u32>,
    global_names: Vec<String>,
    pages_storage_prefix: Vec<u8>,
) -> Result<(), InitError> {
    init_with_handler::<DefaultUserSignalHandler>(
        version,
        page_sizes,
        global_names,
        pages_storage_prefix,
    )
}
