// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
//! In runtime data for program Wasm memory pages can be loaded in lazy manner.
//! All pages, which is supposed to be lazy, must be mprotected before program execution.
//! During execution data from storage is loaded for all pages, which has been accessed
//! and which has data in storage.
//! See also `process::process_lazy_pages`, `signal`, `host_func` for more information.
//!
//! Note: currently we restrict twice write signal from same page during one execution.
//! It's not necessary behavior, but more simple and safe.

#![allow(clippy::items_after_test_module)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod common;
mod globals;
mod host_func;
mod init_flag;
mod mprotect;
mod pages;
mod process;
mod signal;
mod sys;

#[cfg(test)]
mod tests;

pub use common::{Error as LazyPagesError, LazyPagesStorage, LazyPagesVersion};
pub use host_func::pre_process_memory_accesses;
pub use signal::{ExceptionInfo, UserSignalHandler};

use crate::{
    common::{ContextError, CostNo, Costs, LazyPagesContext, PagePrefix, PageSizes},
    globals::{GlobalNo, GlobalsContext},
    init_flag::InitializationFlag,
    pages::{
        GearPagesAmount, GearSizeNo, PagesAmountTrait, SIZES_AMOUNT, SizeNumber, WasmPage,
        WasmPagesAmount, WasmSizeNo,
    },
    signal::DefaultUserSignalHandler,
};
use common::{LazyPagesExecutionContext, LazyPagesRuntimeContext};
use gear_lazy_pages_common::{GlobalsAccessConfig, LazyPagesInitContext, Status};
use mprotect::MprotectError;
use numerated::iterators::IntervalIterator;
use pages::GearPage;
use std::{cell::RefCell, convert::TryInto, num::NonZero};

/// Initialize lazy-pages once for process.
static LAZY_PAGES_INITIALIZED: InitializationFlag = InitializationFlag::new();

thread_local! {
    // NOTE: here we suppose, that each program is executed in separate thread.
    // Or may be in one thread but consequentially.

    static LAZY_PAGES_CONTEXT: RefCell<LazyPagesContext> = RefCell::new(Default::default());
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display("WASM memory native address {_0:#x} is not aligned to the native page size")]
    WasmMemAddrIsNotAligned(usize),
    Mprotect(MprotectError),
    #[from(skip)]
    #[display("Wasm memory end addr is out of usize: begin addr = {_0:#x}, size = {_1:#x}")]
    WasmMemoryEndAddrOverflow(usize, usize),
    #[display("Prefix of storage with memory pages was not set")]
    MemoryPagesPrefixNotSet,
    #[display("Memory size must be null when memory host addr is not set")]
    MemorySizeIsNotNull,
    #[display("Wasm mem size is too big")]
    WasmMemSizeOverflow,
    #[display("Stack end offset cannot be bigger than memory size")]
    StackEndBiggerThanMemSize,
    #[display("Stack end offset is too big")]
    StackEndOverflow,
    #[display("Wasm addr and size are not changed, so host func call is needless")]
    NothingToChange,
    #[display("Wasm memory addr must be set, when trying to change something in lazy pages")]
    WasmMemAddrIsNotSet,
    GlobalContext(ContextError),
    #[from(skip)]
    #[display("Wrong costs amount: get {_0}, must be {_1}")]
    WrongCostsAmount(usize, usize),
}

fn check_memory_interval(addr: usize, size: usize) -> Result<(), Error> {
    addr.checked_add(size)
        .ok_or(Error::WasmMemoryEndAddrOverflow(addr, size))
        .map(|_| ())
}

pub fn initialize_for_program(
    wasm_mem_addr: Option<usize>,
    wasm_mem_size: u32,
    stack_end: Option<u32>,
    program_key: Vec<u8>,
    globals_config: Option<GlobalsAccessConfig>,
    costs: Vec<u64>,
) -> Result<(), Error> {
    // Initialize new execution context
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        let runtime_ctx = ctx.runtime_context_mut()?;

        // Check wasm program memory host address
        if let Some(addr) = wasm_mem_addr
            && !addr.is_multiple_of(region::page::size())
        {
            return Err(Error::WasmMemAddrIsNotAligned(addr));
        }

        // Check stack_end is less or equal than wasm memory size
        let stack_end = stack_end.unwrap_or_default();
        if wasm_mem_size < stack_end {
            return Err(Error::StackEndBiggerThanMemSize);
        }

        let wasm_mem_size =
            WasmPagesAmount::new(runtime_ctx, wasm_mem_size).ok_or(Error::WasmMemSizeOverflow)?;
        let wasm_mem_size_in_bytes = wasm_mem_size.offset(runtime_ctx);

        // Check wasm program memory size
        if let Some(addr) = wasm_mem_addr {
            check_memory_interval(addr, wasm_mem_size_in_bytes)?;
        } else if wasm_mem_size_in_bytes != 0 {
            return Err(Error::MemorySizeIsNotNull);
        }

        let stack_end = WasmPage::new(runtime_ctx, stack_end).ok_or(Error::StackEndOverflow)?;

        let costs: Costs = costs.try_into().map_err(|costs: Vec<u64>| {
            Error::WrongCostsAmount(costs.len(), CostNo::Amount as usize)
        })?;

        let execution_ctx = LazyPagesExecutionContext {
            costs,
            wasm_mem_addr,
            wasm_mem_size,
            program_storage_prefix: PagePrefix::new_from_program_prefix(
                [runtime_ctx.pages_storage_prefix.as_slice(), &program_key].concat(),
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
            let stack_end_offset = execution_ctx.stack_end.offset(runtime_ctx) as usize;
            // `+` and `-` are safe because we checked
            // that `stack_end` is less or equal to `wasm_mem_size` and wasm end addr fits usize.
            let addr = addr + stack_end_offset;
            let size = wasm_mem_size_in_bytes - stack_end_offset;
            if size != 0 {
                mprotect::mprotect_interval(addr, size, false, false)?;
            }
        }

        ctx.set_execution_context(execution_ctx);

        log::trace!("Initialize lazy-pages for current program: {ctx:?}");

        Ok(())
    })
}

/// Protect lazy pages, after they had been unprotected.
pub fn set_lazy_pages_protection() -> Result<(), Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let (rt_ctx, exec_ctx) = ctx.contexts()?;
        let mem_addr = exec_ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;

        // Set r/w protection for all pages except stack pages and write accessed pages.
        let start: GearPage = exec_ctx.stack_end.to_page(rt_ctx);
        let end: GearPagesAmount = exec_ctx.wasm_mem_size.convert(rt_ctx);
        let interval = start.to_end_interval(rt_ctx, end).unwrap_or_else(|| {
            let err_msg = format!(
                "set_lazy_pages_protection: `stack_end` must be less or equal to `wasm_mem_size`. \
                Stack end start - {start:?}, wasm memory size - {end:?}",
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });
        let pages = exec_ctx.write_accessed_pages.voids(interval);
        mprotect::mprotect_pages(mem_addr, pages, rt_ctx, false, false)?;

        // Set only write protection for already accessed, but not write accessed pages.
        let pages = exec_ctx
            .accessed_pages
            .difference(&exec_ctx.write_accessed_pages);
        mprotect::mprotect_pages(mem_addr, pages, rt_ctx, true, false)?;

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
        let (rt_ctx, exec_ctx) = ctx.contexts()?;
        let addr = exec_ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
        let size = exec_ctx.wasm_mem_size.offset(rt_ctx);
        mprotect::mprotect_interval(addr, size, true, true)?;
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
        let (rt_ctx, exec_ctx) = ctx.contexts_mut()?;

        let addr = match addr {
            Some(addr) => match addr % region::page::size() {
                0 => addr,
                _ => return Err(Error::WasmMemAddrIsNotAligned(addr)),
            },

            None => match exec_ctx.wasm_mem_addr {
                Some(addr) => addr,
                None => return Err(Error::WasmMemAddrIsNotSet),
            },
        };

        let size = match size {
            Some(raw) => WasmPagesAmount::new(rt_ctx, raw).ok_or(Error::WasmMemSizeOverflow)?,
            None => exec_ctx.wasm_mem_size,
        };

        check_memory_interval(addr, size.offset(rt_ctx))?;

        exec_ctx.wasm_mem_addr = Some(addr);
        exec_ctx.wasm_mem_size = size;

        Ok(())
    })
}

/// Returns vec of lazy-pages which has been accessed
pub fn write_accessed_pages() -> Result<Vec<u32>, Error> {
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow()
            .execution_context()
            .map(|ctx| {
                ctx.write_accessed_pages
                    .iter()
                    .flat_map(IntervalIterator::from)
                    .map(|p| p.raw())
                    .collect()
            })
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
    #[display("Wrong page sizes amount: get {_0}, must be {_1}")]
    WrongSizesAmount(usize, usize),
    #[display("Wrong global names: expected {_0}, found {_1}")]
    WrongGlobalNames(String, String),
    #[display("Not suitable page sizes")]
    NotSuitablePageSizes,
    #[display("Can not set signal handler: {_0}")]
    CanNotSetUpSignalHandler(String),
    #[display("Failed to init for thread: {_0}")]
    InitForThread(String),
    #[display("Provided by runtime memory page size cannot be zero")]
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

        unsafe extern "C" {
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
        static MACHINE_THREAD_STATE: i32 = x86_THREAD_STATE64;

        // Took const value from https://opensource.apple.com/source/cctools/cctools-870/include/mach/arm/thread_status.h
        // ```
        // #define ARM_THREAD_STATE64		6
        // ```
        #[cfg(target_arch = "aarch64")]
        static MACHINE_THREAD_STATE: i32 = 6;

        unsafe {
            task_set_exception_ports(
                mach_task_self(),
                EXC_MASK_BAD_ACCESS,
                MACH_PORT_NULL,
                EXCEPTION_STATE_IDENTITY as exception_behavior_t,
                MACHINE_THREAD_STATE,
            )
        };
    }

    LAZY_PAGES_INITIALIZED.get_or_init(|| {
        if let Err(err) = unsafe { sys::setup_signal_handler::<H>() } {
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
pub fn init_with_handler<H: UserSignalHandler, S: LazyPagesStorage + 'static>(
    _version: LazyPagesVersion,
    ctx: LazyPagesInitContext,
    pages_storage: S,
) -> Result<(), InitError> {
    use InitError::*;

    let LazyPagesInitContext {
        page_sizes,
        global_names,
        pages_storage_prefix,
    } = ctx;

    // Check that sizes are not zero
    let page_sizes = page_sizes
        .into_iter()
        .map(TryInto::<NonZero<u32>>::try_into)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ZeroPageSize)?;

    let page_sizes: PageSizes = match page_sizes.try_into() {
        Ok(sizes) => sizes,
        Err(sizes) => return Err(WrongSizesAmount(sizes.len(), SIZES_AMOUNT)),
    };

    // Check sizes suitability
    let wasm_page_size = page_sizes[WasmSizeNo::SIZE_NO];
    let gear_page_size = page_sizes[GearSizeNo::SIZE_NO];
    let native_page_size = region::page::size();
    if wasm_page_size < gear_page_size
        || (gear_page_size.get() as usize) < native_page_size
        || !u32::is_power_of_two(wasm_page_size.get())
        || !u32::is_power_of_two(gear_page_size.get())
        || !usize::is_power_of_two(native_page_size)
    {
        return Err(NotSuitablePageSizes);
    }

    // TODO: check globals from context issue #3057
    // we only need to check the globals that are used to keep the state consistent in older runtimes.
    if global_names[GlobalNo::Gas as usize].as_str() != "gear_gas" {
        return Err(WrongGlobalNames(
            "gear_gas".to_string(),
            global_names[GlobalNo::Gas as usize].to_string(),
        ));
    }

    // Set version even if it has been already set, because it can be changed after runtime upgrade.
    LAZY_PAGES_CONTEXT.with(|ctx| {
        ctx.borrow_mut()
            .set_runtime_context(LazyPagesRuntimeContext {
                page_sizes,
                global_names,
                pages_storage_prefix,
                program_storage: Box::new(pages_storage),
            })
    });

    // TODO: remove after usage of `wasmer::Store::set_trap_handler` for lazy-pages
    // we capture executor signal handler first to call it later
    // if our handler is not effective
    wasmer_vm::init_traps();

    unsafe { init_for_process::<H>()? }

    unsafe { sys::init_for_thread().map_err(InitForThread)? }

    Ok(())
}

pub fn init<S: LazyPagesStorage + 'static>(
    version: LazyPagesVersion,
    ctx: LazyPagesInitContext,
    pages_storage: S,
) -> Result<(), InitError> {
    init_with_handler::<DefaultUserSignalHandler, S>(version, ctx, pages_storage)
}
