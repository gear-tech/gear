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

use crate::{
    mprotect, utils, Error, GranularityPage, LazyPage, LazyPagesExecutionContext,
    LAZY_PAGES_PROGRAM_CONTEXT,
};
use cfg_if::cfg_if;
use core::any::Any;
use gear_backend_common::lazy_pages::{
    ChargeForPages, GlobalsAccessError, GlobalsAccessMod, GlobalsAccessor, GlobalsConfig, Status,
};
use gear_core::{
    costs::CostPerPage,
    memory::{GearPage, PageU32Size, PagesIterInclusive, GEAR_PAGE_SIZE, PAGE_STORAGE_GRANULARITY},
};
use sc_executor_common::sandbox::SandboxInstance;
use sp_wasm_interface::Value;
use std::{
    cell::RefMut,
    collections::BTreeSet,
    convert::{TryFrom, TryInto},
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
    pub fn calc_key_for_page(&mut self, page: GearPage) -> &[u8] {
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

/// Accessed pages information.
pub(crate) enum AccessedPagesInfo {
    #[allow(unused)]
    FromHostFunc(BTreeSet<LazyPage>),
    FromSignal(LazyPage),
}

struct GlobalsAccessWasmRuntime<'a> {
    pub instance: &'a mut SandboxInstance,
}

impl<'a> GlobalsAccessor for GlobalsAccessWasmRuntime<'a> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.instance
            .get_global_val(name)
            .and_then(|value| {
                if let Value::I64(value) = value {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or(GlobalsAccessError)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.instance
            .set_global_val(name, Value::I64(value))
            .ok()
            .flatten()
            .ok_or(GlobalsAccessError)?;
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        unimplemented!("Has no use cases for this struct")
    }
}

struct GlobalsAccessNativeRuntime<'a, 'b> {
    pub inner_access_provider: &'a mut &'b mut dyn GlobalsAccessor,
}

impl<'a, 'b> GlobalsAccessor for GlobalsAccessNativeRuntime<'a, 'b> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.inner_access_provider.get_i64(name)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.inner_access_provider.set_i64(name, value)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        unimplemented!("Has no use cases for this struct")
    }
}

fn charge_gas_internal(
    mut globals_access_provider: impl GlobalsAccessor,
    global_gas: &str,
    global_allowance: &str,
    amount: u64,
) -> Result<Status, Error> {
    let mut sub_global = |name, value| {
        let current_value = globals_access_provider.get_i64(name).ok()? as u64;
        let (new_value, exceed) = current_value
            .checked_sub(value)
            .map(|val| (val, false))
            .unwrap_or((0, true));
        globals_access_provider
            .set_i64(name, new_value as i64)
            .ok()?;

        log::trace!("Change global {name}: {current_value} -> {new_value}, exceeded: {exceed}");

        Some(exceed)
    };
    if sub_global(global_gas, amount).ok_or(Error::CannotChargeGas)? {
        return Ok(Status::GasLimitExceeded);
    }
    if sub_global(global_allowance, amount).ok_or(Error::CannotChargeGasAllowance)? {
        return Ok(Status::GasAllowanceExceeded);
    }
    Ok(Status::Normal)
}

unsafe fn charge_gas(globals_config: &GlobalsConfig, amount: u64) -> Result<Status, Error> {
    match globals_config.globals_access_mod {
        GlobalsAccessMod::WasmRuntime => {
            let instance = (globals_config.globals_access_ptr as *mut SandboxInstance)
                .as_mut()
                .ok_or(Error::HostInstancePointerIsInvalid)?;
            charge_gas_internal(
                GlobalsAccessWasmRuntime { instance },
                &globals_config.global_gas_name,
                &globals_config.global_allowance_name,
                amount,
            )
        }
        GlobalsAccessMod::NativeRuntime => {
            let inner_access_provider = (globals_config.globals_access_ptr
                as *mut &mut dyn GlobalsAccessor)
                .as_mut()
                .ok_or(Error::DynGlobalsAccessPointerIsInvalid)?;
            charge_gas_internal(
                GlobalsAccessNativeRuntime {
                    inner_access_provider,
                },
                &globals_config.global_gas_name,
                &globals_config.global_allowance_name,
                amount,
            )
        }
    }
}

fn cost_for_write(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    page: GranularityPage,
) -> CostPerPage<GranularityPage> {
    if ctx.write_charged.contains(&page) {
        // Has been already charged for write.
        0.into()
    } else if ctx.read_charged.contains(&page) {
        // Has been already charged for read.
        if ctx.write_after_read_charged.contains(&page) {
            // Has been already charged for write after read.
            0.into()
        } else {
            // Charge for write after read.
            ctx.write_after_read_charged.insert(page);
            ctx.lazy_pages_weights.write_after_read
        }
    } else {
        // Charge for write.
        ctx.write_charged.insert(page);
        ctx.lazy_pages_weights.write
    }
}

fn cost_for_read(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    page: GranularityPage,
) -> CostPerPage<GranularityPage> {
    if ctx.read_charged.contains(&page) || ctx.write_charged.contains(&page) {
        // Has been already charged for write or read - so no need to charge for read.
        0.into()
    } else {
        // Charge for read.
        ctx.read_charged.insert(page);
        ctx.lazy_pages_weights.read
    }
}

unsafe fn charge_for_pages(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    pages: PagesIterInclusive<LazyPage>,
    is_write: bool,
) -> Result<Status, Error> {
    if ctx.globals_config.is_none() {
        return Ok(Status::Normal);
    }

    let mut amount = 0u64;
    let granularity_pages: BTreeSet<GranularityPage> = pages.map(|page| page.to_page()).collect();

    for page in granularity_pages.into_iter() {
        let amount_for_page = if is_write {
            cost_for_write(ctx, page)
        } else {
            cost_for_read(ctx, page)
        };
        amount = amount.saturating_add(amount_for_page.calc(1.into()));
    }

    if amount != 0 {
        // Panic is impossible, because we checked above: `ctx.globals_config` is `Some`.
        let globals_config = ctx
            .globals_config
            .as_ref()
            .unwrap_or_else(|| unreachable!("Globals config is `None`"));
        charge_gas(globals_config, amount)
    } else {
        Ok(Status::Normal)
    }
}

unsafe fn charge_for_load_storage_data(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    page: LazyPage,
) -> Result<Status, Error> {
    if ctx.globals_config.is_none() {
        return Ok(Status::Normal);
    }

    if ctx.read_storage_data_charged.insert(page.to_page()) {
        // Charge for read from storage.

        let amount = ctx.lazy_pages_weights.load_page_storage_data.calc(1.into());

        // Panic is impossible, because we checked above: `ctx.globals_config` is `Some`.
        let globals_config = ctx
            .globals_config
            .as_ref()
            .unwrap_or_else(|| unreachable!("Globals config is `None`"));

        charge_gas(globals_config, amount)
    } else {
        // Already charged for load from storage

        Ok(Status::Normal)
    }
}

pub(crate) unsafe fn process_lazy_pages(
    mut ctx: RefMut<LazyPagesExecutionContext>,
    accessed_pages: AccessedPagesInfo,
    is_write: bool,
) -> Result<Option<ChargeForPages>, Error> {
    let wasm_mem_size = ctx.wasm_mem_size.ok_or(Error::WasmMemSizeIsNotSet)?;

    let (last_page, is_signal) = match &accessed_pages {
        AccessedPagesInfo::FromHostFunc(accessed_pages) => (accessed_pages.last(), false),
        AccessedPagesInfo::FromSignal(lazy_page) => (Some(lazy_page), true),
    };

    if let Some(last_page) = last_page {
        // Check that all pages are inside wasm memory.
        if last_page.end_offset() >= wasm_mem_size.offset() {
            return Err(Error::OutOfWasmMemoryAccess);
        }
    } else {
        // Accessed pages are empty - nothing to do.
        return Ok(None);
    }

    let status = ctx.status.as_ref().ok_or(Error::StatusIsNone)?;
    match status {
        Status::Normal => {}
        Status::GasLimitExceeded | Status::GasAllowanceExceeded => {
            if is_signal {
                // Because we unprotect all lazy-pages when status is `exceeded`, then
                // we cannot receive signals from wasm memory until the end of execution.
                return Err(Error::SignalWhenStatusGasExceeded);
            } else {
                // Currently, we charge gas for sys-call after memory processing, so this can appear.
                // In this case we do nothing, because all memory is already unprotected, and no need
                // to take in account pages data from storage, because gas is exceeded.
                return Ok(None);
            }
        }
    }

    let stack_end = ctx.stack_end_wasm_page;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
    let mut prefix = PagePrefix::new_from_program_prefix(
        ctx.program_storage_prefix
            .as_ref()
            .ok_or(Error::ProgramPrefixIsNotSet)?,
    );

    // Returns `true` if new status is not `Normal`.
    let update_status = |ctx: &mut RefMut<LazyPagesExecutionContext>, status| {
        ctx.status.replace(status);

        // If new status is not [Status::Normal], then unprotect lazy-pages
        // and continue work until the end of current wasm block. We don't care
        // about future contract execution correctness, because gas limit or allowance exceed.
        match status {
            Status::Normal => Ok(false),
            Status::GasLimitExceeded | Status::GasAllowanceExceeded => {
                log::trace!(
                    "Gas limit or allowance exceed, so removes protection from all wasm memory \
                    and continues execution until the end of current wasm block"
                );
                mprotect::mprotect_interval(
                    wasm_mem_addr,
                    wasm_mem_size.offset() as usize,
                    true,
                    true,
                )
                .map(|_| true)
            }
        }
    };

    let mut charge_set = ChargeForPages::default();

    let mut f = |pages: PagesIterInclusive<LazyPage>| {
        let psg = PAGE_STORAGE_GRANULARITY as u32;
        let Some(mut start) = pages.current() else {
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

        if is_signal {
            // If it's signal, then need to charge for accessed pages.
            let status = charge_for_pages(&mut ctx, pages.clone(), is_write)?;

            if update_status(&mut ctx, status)? {
                return Ok(());
            }
        }

        for lazy_page in pages {
            let granularity_page = lazy_page.to_page();
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
            } else if ctx.accessed_pages.contains(&lazy_page) {
                if is_write {
                    // Set read/write access for page and add page to released.
                    mprotect::mprotect_interval(
                        wasm_mem_addr + lazy_page.offset() as usize,
                        LazyPage::size() as usize,
                        true,
                        true,
                    )?;
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                    if !is_signal && ctx.write_after_read_charged.insert(granularity_page) {
                        if !ctx.read_charged.contains(&granularity_page)
                            || ctx.write_charged.contains(&granularity_page)
                        {
                            unreachable!("Lazy-pages context charge sets are in incorrect state");
                        }
                        charge_set.write_accessed = charge_set.write_accessed.inc().unwrap();
                    }
                } else {
                    // Nothing to do, page has read accesses and data is in correct state.
                    if is_signal {
                        return Err(Error::ReadAccessSignalFromAccessedPage);
                    }
                }
            } else {
                if is_signal {
                    // If it's signal we need charge for loading page data from storage.
                    let status = charge_for_load_storage_data(&mut ctx, lazy_page)?;

                    if update_status(&mut ctx, status)? {
                        return Ok(());
                    }
                } else {
                    if ctx.read_storage_data_charged.insert(lazy_page.to_page()) {
                        charge_set.read_storage_data = charge_set.read_storage_data.inc().unwrap();
                    }
                    if is_write && ctx.write_charged.insert(lazy_page.to_page()) {
                        if ctx.read_charged.contains(&granularity_page)
                            || ctx.write_after_read_charged.contains(&granularity_page)
                        {
                            unreachable!("Lazy-pages context charge sets are in incorrect state");
                        }
                        charge_set.write_accessed = charge_set.write_accessed.inc().unwrap();
                    }
                }

                // Need to set read/write access.
                mprotect::mprotect_interval(
                    wasm_mem_addr + lazy_page.offset() as usize,
                    LazyPage::size() as usize,
                    true,
                    true,
                )?;

                // Download data for `lazy_page` from storage
                for gear_page in lazy_page.to_pages_iter::<GearPage>() {
                    let page_buffer_ptr =
                        (wasm_mem_addr as *mut u8).add(gear_page.offset() as usize);
                    let buffer_as_slice =
                        std::slice::from_raw_parts_mut(page_buffer_ptr, GearPage::size() as usize);
                    let res = sp_io::storage::read(
                        prefix.calc_key_for_page(gear_page),
                        buffer_as_slice,
                        0,
                    );

                    log::trace!("{:?} has data in storage: {}", gear_page, res.is_some());

                    // Check data size is valid.
                    if let Some(size) = res.filter(|&size| size != GearPage::size()) {
                        return Err(Error::InvalidPageDataSize {
                            expected: GearPage::size(),
                            actual: size,
                        });
                    }
                }

                // And add `lazy_page` to accessed pages.
                ctx.accessed_pages.insert(lazy_page);

                if is_write {
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                } else {
                    // Set only read access for page.
                    mprotect::mprotect_interval(
                        wasm_mem_addr + lazy_page.offset() as usize,
                        LazyPage::size() as usize,
                        true,
                        false,
                    )?;
                }
            }
        }

        Ok(())
    };

    match accessed_pages {
        AccessedPagesInfo::FromHostFunc(accessed_pages) => {
            utils::with_inclusive_ranges(&accessed_pages, f)?;
        }
        AccessedPagesInfo::FromSignal(lazy_page) => f(lazy_page.iter_once())?,
    }

    Ok(Some(charge_set))
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
    process_lazy_pages(ctx, AccessedPagesInfo::FromSignal(lazy_page), is_write).map(|_| ())
}

/// User signal handler. Logic can depends on lazy-pages version.
/// For the most recent logic see "self::user_signal_handler_internal"
pub(crate) unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_PROGRAM_CONTEXT.with(|ctx| user_signal_handler_internal(ctx.borrow_mut(), info))
}
