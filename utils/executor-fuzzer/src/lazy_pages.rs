// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    mem,
    ops::Range,
    ptr,
};

use gear_lazy_pages::{
    ExceptionInfo, LazyPagesError as Error, LazyPagesVersion, UserSignalHandler,
};
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_wasm_instrument::GLOBAL_NAME_GAS;

use crate::{globals::InstanceAccessGlobal, OS_PAGE_SIZE};

pub type HostPageAddr = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchedPage {
    pub write: bool,
    pub read: bool,
}

impl TouchedPage {
    fn update(&mut self, other: &Self) {
        self.write |= other.write;
        self.read |= other.read;
    }
}

pub struct FuzzerLazyPagesContext {
    pub memory_range: Range<usize>,
    pub instance: Box<dyn InstanceAccessGlobal>,
    pub pages: HashMap<HostPageAddr, TouchedPage>,
    //globals_update_list: GlobalList,
}

thread_local! {
    static FUZZER_LP_CONTEXT: RefCell<Option<FuzzerLazyPagesContext>> = const { RefCell::new(None) };
}

pub fn init_fuzzer_lazy_pages(init: FuzzerLazyPagesContext) {
    const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"dummydummydummydummydummydummy01";

    let mem_range = init.memory_range.clone();

    FUZZER_LP_CONTEXT.with(|ctx: &RefCell<Option<FuzzerLazyPagesContext>>| {
        *ctx.borrow_mut() = Some(init);
    });

    unsafe {
        mprotect_interval(
            mem_range.start,
            mem_range.end - mem_range.start,
            false,
            false,
            false,
        )
        .expect("failed to protect memory")
    }

    gear_lazy_pages::init_with_handler::<FuzzerLazyPagesSignalHandler, ()>(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(PROGRAM_STORAGE_PREFIX),
        (),
    )
    .expect("Failed to init lazy-pages");
}

pub fn get_touched_pages() -> BTreeMap<HostPageAddr, (TouchedPage, Vec<u8>)> {
    let pages = FUZZER_LP_CONTEXT.with(|ctx: &RefCell<Option<FuzzerLazyPagesContext>>| {
        let mut borrow = ctx.borrow_mut();
        let ctx = borrow.as_mut().expect("lazy pages initialized");
        mem::take(&mut ctx.pages)
    });

    pages
        .into_iter()
        .map(|(addr, page)| {
            let mut data = vec![0; OS_PAGE_SIZE];

            // Unprotect page for read
            if !page.read {
                unsafe {
                    mprotect_interval(addr, OS_PAGE_SIZE, true, false, false)
                        .expect("unprotect page");
                }
            }

            // SAFETY: these pages still allocated by VM and not freed.
            unsafe {
                ptr::copy_nonoverlapping(addr as *const u8, data.as_mut_ptr(), OS_PAGE_SIZE);
            }

            (addr, (page, data))
        })
        .collect()
}

struct FuzzerLazyPagesSignalHandler;

impl UserSignalHandler for FuzzerLazyPagesSignalHandler {
    unsafe fn handle(info: ExceptionInfo) -> std::result::Result<(), Error> {
        log::debug!("Interrupted, exception info = {:?}", info);
        FUZZER_LP_CONTEXT.with(|ctx| {
            let mut borrow = ctx.borrow_mut();
            let ctx = borrow.as_mut().ok_or_else(|| Error::WasmMemAddrIsNotSet)?;
            user_signal_handler_internal(ctx, info)
        })
    }
}

fn user_signal_handler_internal(
    ctx: &mut FuzzerLazyPagesContext,
    info: ExceptionInfo,
) -> Result<(), Error> {
    let native_addr = info.fault_addr as usize;
    let is_write = info.is_write.ok_or_else(|| Error::ReadOrWriteIsUnknown)?;
    let wasm_mem_range = &ctx.memory_range;

    if !wasm_mem_range.contains(&native_addr) {
        return Err(Error::OutOfWasmMemoryAccess);
    }

    log::trace!(
        "SIG: Unprotect WASM memory at address: {:#x}, wr: {is_write}",
        native_addr
    );

    unsafe {
        mprotect_interval(native_addr, OS_PAGE_SIZE, true, is_write, false).unwrap();
    }

    // Update touched pages
    let page = TouchedPage {
        write: is_write,
        read: !is_write,
    };
    ctx.pages
        .entry(native_addr)
        .and_modify(|prev_access| {
            prev_access.update(&page);
        })
        .or_insert(page);

    // Update gas global
    let mut gas = ctx
        .instance
        .get_global(GLOBAL_NAME_GAS)
        .expect("global get");
    gas = gas.saturating_sub(100);
    ctx.instance
        .set_global(GLOBAL_NAME_GAS, gas)
        .expect("global set");

    Ok(())
}

/// `mprotect` native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments.
unsafe fn mprotect_interval(
    addr: usize,
    size: usize,
    allow_read: bool,
    allow_write: bool,
    allow_exec: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if size == 0 {
        panic!("zero size is restricted for mprotect");
    }

    let mut mask = region::Protection::NONE;
    if allow_read {
        mask |= region::Protection::READ;
    }
    if allow_write {
        mask |= region::Protection::WRITE;
    }
    if allow_exec {
        mask |= region::Protection::EXECUTE;
    }
    region::protect(addr as *mut (), size, mask)?;
    log::trace!("mprotect interval: {addr:#x}, size: {size:#x}, mask: {mask}");
    Ok(())
}
