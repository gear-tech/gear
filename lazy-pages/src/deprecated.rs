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

//! Deprecated lazy-pages impl to support old runtimes.

use region::Protection;
use std::cell::RefMut;

use crate::{sys::ExceptionInfo, Error, LazyPagesExecutionContext, PageBuf, PageNumber};

/// Returns key which `page` has in storage.
/// `prefix` is current program prefix in storage.
#[deprecated]
fn page_key_in_storage(prefix: &Vec<u8>, page: PageNumber) -> Vec<u8> {
    let mut key = Vec::with_capacity(prefix.len() + std::mem::size_of::<u32>());
    key.extend(prefix);
    key.extend(page.0.to_le_bytes());
    key
}

#[deprecated]
pub(crate) unsafe fn user_signal_handler_internal_v1(
    mut ctx: RefMut<LazyPagesExecutionContext>,
    info: ExceptionInfo,
) -> Result<(), Error> {
    let native_ps = region::page::size();
    let gear_ps = PageNumber::size();

    log::debug!("Interrupted, exception info = {:?}", info);

    let mem = info.fault_addr;
    let native_page = region::page::floor(mem) as usize;
    let wasm_mem_begin = ctx.wasm_mem_addr.ok_or(Error::WasmMemAddrIsNotSet)? as usize;

    if wasm_mem_begin > native_page {
        return Err(Error::SignalAddrIsLessThenWasmMemAddr {
            addr: native_page,
            wasm_mem_addr: wasm_mem_begin,
        });
    }

    // First gear page which must be unprotected
    let gear_page = PageNumber(((native_page - wasm_mem_begin) / gear_ps) as u32);

    let (gear_page, gear_pages_num, unprot_addr) = if native_ps > gear_ps {
        assert_eq!(native_ps % gear_ps, 0);
        (gear_page, native_ps / gear_ps, native_page)
    } else {
        assert_eq!(gear_ps % native_ps, 0);
        (gear_page, 1usize, wasm_mem_begin + gear_page.offset())
    };

    let accessed_page = PageNumber(((mem as usize - wasm_mem_begin) / gear_ps) as u32);
    log::debug!(
        "mem={:?} accessed={:?},{:?} pages={:?} page_native_addr={:#x}",
        mem,
        accessed_page,
        accessed_page.to_wasm_page(),
        gear_page.0..gear_page.0 + gear_pages_num as u32,
        unprot_addr
    );

    let unprot_size = gear_pages_num * gear_ps;

    region::protect(unprot_addr as *mut (), unprot_size, Protection::READ_WRITE)?;

    for idx in 0..gear_pages_num as u32 {
        let page = gear_page + idx.into();

        let ptr = (unprot_addr as *mut u8).add(idx as usize * gear_ps);
        let buffer_as_slice = std::slice::from_raw_parts_mut(ptr, gear_ps);

        // TODO: simplify before release (issue #1147). Currently we must support here all old runtimes.
        // For new runtimes we have to calc page key from program pages prefix.
        let page_key = if let Some(prefix) = &ctx.program_storage_prefix {
            page_key_in_storage(prefix, page)
        } else {
            // This case is for old runtimes support
            ctx.lazy_pages_info
                .remove(&page)
                .ok_or(Error::LazyPageNotExistForSignalAddr(mem, page))?
        };
        let res = sp_io::storage::read(&page_key, buffer_as_slice, 0);

        if res.is_none() {
            log::trace!(
                "{:?} has no data in storage, so just save current page data to released pages",
                page
            );
        } else {
            log::trace!(
                "{:?} has data in storage, so set this data for page and save it in released pages",
                page
            );
        }

        if let Some(size) = res.filter(|&size| size as usize != PageNumber::size()) {
            return Err(Error::InvalidPageDataSize {
                expected: PageNumber::size(),
                actual: size,
            });
        }

        let page_buf = PageBuf::new_from_vec(buffer_as_slice.to_vec())
            .expect("Cannot panic here, because we create slice with PageBuf size");

        let _ = ctx.released_lazy_pages.insert(page);
        if ctx
            .released_lazy_pages_old
            .insert(page, Some(page_buf))
            .is_some()
        {
            return Err(Error::DoubleRelease(page));
        }
    }
    Ok(())
}
