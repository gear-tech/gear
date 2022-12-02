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

use cfg_if::cfg_if;
use core::any::Any;
use region::Protection;
use sc_executor_common::sandbox::SandboxInstance;
use sp_wasm_interface::Value;
use std::{
    cell::RefMut, collections::BTreeSet, convert::TryFrom, iter::FromIterator, ops::RangeInclusive,
};

use crate::{
    utils::with_inclusive_ranges, Error, GranularityPage, LazyPage, LazyPagesExecutionContext,
    LAZY_PAGES_CONTEXT,
};

use gear_core::{
    lazy_pages::{GlobalsAccessError, GlobalsAccessMod, GlobalsAccessTrait, GlobalsCtx, Status},
    memory::{to_page_iter, PageNumber, PageU32Size, GEAR_PAGE_SIZE, PAGE_STORAGE_GRANULARITY},
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

pub mod mprotect;

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
    pub fn calc_key_for_page(&mut self, page: PageNumber) -> &[u8] {
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

struct GlobalsAccessSandbox<'a> {
    pub instance: &'a mut SandboxInstance,
}

impl<'a> GlobalsAccessTrait for GlobalsAccessSandbox<'a> {
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

    fn get_i32(&self, _name: &str) -> Result<i32, GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn set_i32(&mut self, _name: &str, _value: i32) -> Result<(), GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        unreachable!()
    }
}

struct GlobalsAccessDyn<'a, 'b> {
    pub inner_access_provider: &'a mut &'b mut dyn GlobalsAccessTrait,
}

impl<'a, 'b> GlobalsAccessTrait for GlobalsAccessDyn<'a, 'b> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.inner_access_provider.get_i64(name)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.inner_access_provider.set_i64(name, value)
    }

    fn get_i32(&self, _name: &str) -> Result<i32, GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn set_i32(&mut self, _name: &str, _value: i32) -> Result<(), GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        unreachable!()
    }
}

fn charge_gas_internal(
    mut globals_access_provider: impl GlobalsAccessTrait,
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

unsafe fn charge_gas(
    globals_ctx: Option<&GlobalsCtx>,
    gear_pages_amount: u32,
    is_write: bool,
    is_second_access: bool,
) -> Result<Status, Error> {
    let globals_ctx = if let Some(ctx) = globals_ctx {
        ctx
    } else {
        return Ok(Status::Normal);
    };
    let amount = match (is_write, is_second_access) {
        (false, _) => globals_ctx.lazy_pages_weights.read,
        (true, false) => globals_ctx.lazy_pages_weights.write,
        (true, true) => globals_ctx.lazy_pages_weights.write_after_read,
    };
    let amount = amount.saturating_mul(gear_pages_amount as u64);
    match globals_ctx.globals_access_mod {
        GlobalsAccessMod::WasmRuntime => {
            let instance = (globals_ctx.globals_access_ptr as *mut SandboxInstance)
                .as_mut()
                .ok_or(Error::HostInstancePointerIsInvalid)?;
            charge_gas_internal(
                GlobalsAccessSandbox { instance },
                &globals_ctx.global_gas_name,
                &globals_ctx.global_allowance_name,
                amount,
            )
        }
        GlobalsAccessMod::NativeRuntime => {
            let inner_access_provider = (globals_ctx.globals_access_ptr
                as *mut &mut dyn GlobalsAccessTrait)
                .as_mut()
                .ok_or(Error::DynGlobalsAccessPointerIsInvalid)?;
            charge_gas_internal(
                GlobalsAccessDyn {
                    inner_access_provider,
                },
                &globals_ctx.global_gas_name,
                &globals_ctx.global_allowance_name,
                amount,
            )
        }
    }
}

fn process_status(status: Status) -> Option<()> {
    match status {
        Status::Normal => Some(()),
        Status::GasLimitExceeded | Status::GasAllowanceExceeded => {
            log::trace!("Gas limit or allowance exceed, so set exceed status and work in this mod until the end of execution");
            None
        }
    }
}

pub(crate) unsafe fn process_lazy_pages(
    mut ctx: RefMut<LazyPagesExecutionContext>,
    accessed_pages: BTreeSet<LazyPage>,
    is_write: bool,
    is_signal: bool,
) -> Result<(), Error> {
    let wasm_mem_size = ctx.wasm_mem_size.ok_or(Error::WasmMemSizeIsNotSet)?;

    if let Some(last_page) = accessed_pages.last() {
        // Check that all pages are inside wasm memory.
        if last_page.end_offset() >= wasm_mem_size.offset() {
            return Err(Error::OutOfWasmMemoryAccess);
        }
    } else {
        // Accessed pages are empty - nothing to do.
        return Ok(());
    }

    let stack_end = ctx.stack_end_wasm_page;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;
    let mut prefix = PagePrefix::new_from_program_prefix(
        ctx.program_storage_prefix
            .as_ref()
            .ok_or(Error::ProgramPrefixIsNotSet)?,
    );

    let f = |pages: RangeInclusive<LazyPage>| {
        let psg = PAGE_STORAGE_GRANULARITY as u32;
        let mut start = *pages.start();
        let mut end = *pages.end();

        // Extend pages interval, if start or end access pages, which has no data in storage.
        if is_write && LazyPage::size() < psg {
            if !sp_io::storage::exists(prefix.calc_key_for_page(start.to_page())) {
                start = start.align_down(psg);
            }
            if !sp_io::storage::exists(prefix.calc_key_for_page(end.to_page())) {
                // Make page end aligned to `psg` for `end`.
                // This operations are safe, because `psg` is power of two and smaller then `u32::MAX`.
                // `LazyPage::size()` is less or equal then `psg` and `psg % LazyPage::size() == 0`.
                end = LazyPage::from_offset((end.offset() / psg) * psg + (psg - LazyPage::size()));
            }
        }

        for lazy_page in start..=end {
            let granularity_page = GranularityPage::from_offset(lazy_page.offset());
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
            } else if ctx.accessed_lazy_pages.contains(&lazy_page) {
                if is_write {
                    if is_signal && !ctx.read_after_write_charged.contains(&granularity_page) {
                        // Charge gas for "write after read", because page has been already read accessed.
                        let status = charge_gas(
                            ctx.globals_ctx.as_ref(),
                            GranularityPage::size() / PageNumber::size(),
                            true,
                            true,
                        )?;
                        ctx.status.replace(status);
                        if process_status(status).is_none() {
                            return Ok(());
                        }
                        ctx.read_after_write_charged.insert(granularity_page);
                    }
                    // Set read/write access for page and add page to released.
                    region::protect(
                        (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                        LazyPage::size() as usize,
                        Protection::READ_WRITE,
                    )?;
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                } else {
                    // Nothing to do, page has read accesses and data is in correct state.
                    if is_signal {
                        return Err(Error::ReadAccessSignalFromAccessedPage);
                    }
                }
            } else {
                if is_signal
                    && ((is_write && !ctx.write_charged.contains(&granularity_page))
                        || (!is_write && !ctx.read_charged.contains(&granularity_page)))
                {
                    let status = charge_gas(
                        ctx.globals_ctx.as_ref(),
                        GranularityPage::size() / PageNumber::size(),
                        is_write,
                        false,
                    )?;
                    ctx.status.replace(status);
                    if process_status(status).is_none() {
                        return Ok(());
                    }
                    if is_write {
                        ctx.write_charged.insert(granularity_page);
                    } else {
                        ctx.read_charged.insert(granularity_page);
                    }
                }

                // Need to set read/write access,
                // download data for `lazy_page` from storage and add `lazy_page` to accessed pages.
                region::protect(
                    (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                    LazyPage::size() as usize,
                    Protection::READ_WRITE,
                )?;

                for gear_page in to_page_iter::<_, PageNumber>(lazy_page) {
                    let page_buffer_ptr =
                        (wasm_mem_addr as *mut u8).add(gear_page.offset() as usize);
                    let buffer_as_slice = std::slice::from_raw_parts_mut(
                        page_buffer_ptr,
                        PageNumber::size() as usize,
                    );
                    let res = sp_io::storage::read(
                        prefix.calc_key_for_page(gear_page),
                        buffer_as_slice,
                        0,
                    );

                    log::trace!("{:?} has data in storage: {}", gear_page, res.is_some());

                    // Check data size is valid.
                    if let Some(size) = res.filter(|&size| size != PageNumber::size()) {
                        return Err(Error::InvalidPageDataSize {
                            expected: PageNumber::size(),
                            actual: size,
                        });
                    }
                }

                ctx.accessed_lazy_pages.insert(lazy_page);

                if is_write {
                    log::trace!("add {lazy_page:?} to released");
                    if !ctx.released_pages.insert(lazy_page) {
                        return Err(Error::DoubleRelease(lazy_page));
                    }
                } else {
                    // Set only read access for page.
                    region::protect(
                        (wasm_mem_addr + lazy_page.offset() as usize) as *mut (),
                        LazyPage::size() as usize,
                        Protection::READ,
                    )?;
                }
            }
        }

        Ok(())
    };

    match with_inclusive_ranges(accessed_pages.into_iter(), f) {
        Err(err) => Err(Error::Other(err.to_string())),
        Ok(res) => res,
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
    ctx: RefMut<LazyPagesExecutionContext>,
    info: ExceptionInfo,
) -> Result<(), Error> {
    let status = ctx.status.as_ref().ok_or(Error::StatusIsNone)?;
    match status {
        Status::Normal => {}
        Status::GasLimitExceeded | Status::GasAllowanceExceeded => return Ok(()),
    }

    let native_addr = info.fault_addr as usize;
    let is_write = info.is_write.ok_or(Error::ReadOrWriteIsUnknown)?;
    let wasm_mem_addr = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)?;

    if native_addr < wasm_mem_addr {
        return Err(Error::OutOfWasmMemoryAccess);
    }

    let offset =
        u32::try_from(native_addr - wasm_mem_addr).map_err(|_| Error::OutOfWasmMemoryAccess)?;
    let lazy_page = LazyPage::from_offset(offset);
    let accessed_pages = BTreeSet::from_iter(std::iter::once(lazy_page));
    process_lazy_pages(ctx, accessed_pages, is_write, true)
}

/// User signal handler. Logic can depends on lazy-pages version.
/// For the most recent logic see "self::user_signal_handler_internal"
pub(crate) unsafe fn user_signal_handler(info: ExceptionInfo) -> Result<(), Error> {
    log::debug!("Interrupted, exception info = {:?}", info);
    LAZY_PAGES_CONTEXT.with(|ctx| user_signal_handler_internal(ctx.borrow_mut(), info))
}
