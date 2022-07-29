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

//! Runtime interface for gear node

// TODO: remove all deprecated code before release (issue #1147)

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(useless_deprecated, deprecated)]

use codec::{Decode, Encode};
use core::ops::RangeInclusive;
use gear_core::memory::{HostPointer, PageBuf};
use sp_runtime_interface::runtime_interface;

mod deprecated;
use deprecated::*;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[cfg(feature = "std")]
use gear_core::memory::PageNumber;
#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{result::Result, vec::Vec};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum RIError {
    #[display(fmt = "Cannot mprotect interval: {:?}, mask = {}", interval, mask)]
    MprotectError {
        interval: RangeInclusive<u64>,
        mask: u64,
    },
    #[display(
        fmt = "Memory interval size {:#x} for protection is not aligned by native page size {:#x}",
        size,
        page_size
    )]
    MprotectSizeError { size: u64, page_size: u64 },
    #[display(fmt = "Unsupported OS")]
    UnsupportedOS,
    #[display(
        fmt = "Wasm memory buffer addr {:#x} is not aligned by native page size {:#x}",
        addr,
        page_size
    )]
    WasmMemBufferNotAligned { addr: u64, page_size: u64 },
}

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments.
#[cfg(feature = "std")]
pub(crate) unsafe fn sys_mprotect_interval(
    addr: HostPointer,
    size: usize,
    prot_read: bool,
    prot_write: bool,
    prot_exec: bool,
) -> Result<(), RIError> {
    if size == 0 || size % region::page::size() != 0 {
        return Err(RIError::MprotectSizeError {
            size: size as u64,
            page_size: region::page::size() as u64,
        });
    }

    let mut prot_mask = region::Protection::NONE;
    if prot_read {
        prot_mask |= region::Protection::READ;
    }
    if prot_write {
        prot_mask |= region::Protection::WRITE;
    }
    if prot_exec {
        prot_mask |= region::Protection::EXECUTE;
    }
    let res = region::protect(addr as *mut (), size, prot_mask);
    if let Err(err) = res {
        log::error!(
            "Cannot set page protection for addr={:#x} size={:#x} mask={}: {}",
            addr,
            size,
            prot_mask,
            err,
        );
        return Err(RIError::MprotectError {
            interval: addr..=addr + size as u64,
            mask: prot_mask.bits() as u64,
        });
    }
    log::trace!(
        "mprotect native mem interval: {:#x}, size: {:#x}, mask: {}",
        addr,
        size,
        prot_mask
    );
    Ok(())
}

/// Protect all pages in memory interval, except pages from `except_pages`.
/// If `protect` is true then restrict read/write access, else allow them.
#[cfg(feature = "std")]
fn mprotect_mem_interval_except_pages(
    mem_addr: HostPointer,
    start_offset: u32,
    mem_size: usize,
    except_pages: impl Iterator<Item = PageNumber>,
    protect: bool,
) -> Result<(), RIError> {
    let mprotect = |start, end| {
        let addr = mem_addr + start as HostPointer;
        let size = end - start;
        unsafe { sys_mprotect_interval(addr, size, !protect, !protect, false) }
    };

    // TODO: remove panics and make an errors (issue #1147)
    assert!(start_offset as usize <= mem_size);

    let mut interval_offset = start_offset as usize;
    for page in except_pages {
        let page_offset = page.offset();
        if page_offset > interval_offset {
            mprotect(interval_offset, page_offset)?;
        }
        interval_offset = page_offset.saturating_add(PageNumber::size());
    }
    if mem_size > interval_offset {
        mprotect(interval_offset, mem_size)
    } else {
        Ok(())
    }
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    #[deprecated]
    fn mprotect_wasm_pages(
        from_ptr: u64,
        pages_nums: &[u32],
        prot_read: bool,
        prot_write: bool,
        prot_exec: bool,
    ) {
        unsafe {
            let _ = sys_mprotect_wasm_pages(from_ptr, pages_nums, prot_read, prot_write, prot_exec);
        }
    }

    #[version(2)]
    #[deprecated]
    fn mprotect_wasm_pages(
        from_ptr: u64,
        pages_nums: &[u32],
        prot_read: bool,
        prot_write: bool,
        prot_exec: bool,
    ) -> Result<(), MprotectError> {
        unsafe { sys_mprotect_wasm_pages(from_ptr, pages_nums, prot_read, prot_write, prot_exec) }
    }

    #[deprecated]
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), MprotectError> {
        let lazy_pages = lazy_pages::get_lazy_pages_numbers();
        mprotect_pages_slice(wasm_mem_addr, &lazy_pages, protect).map_err(|err| match err {
            RIError::UnsupportedOS => MprotectError::OsError,
            _ => MprotectError::PageError,
        })
    }

    #[deprecated]
    #[version(2)]
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), RIError> {
        let lazy_pages = lazy_pages::get_lazy_pages_numbers();
        mprotect_pages_slice(wasm_mem_addr, &lazy_pages, protect)
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    #[version(3)]
    fn mprotect_lazy_pages(protect: bool) -> Result<(), RIError> {
        log::trace!("mem size = {:?}", lazy_pages::get_wasm_mem_size());
        mprotect_mem_interval_except_pages(
            // TODO: remove panics and make an errors (issue #1147)
            lazy_pages::get_wasm_mem_addr()
                .expect("Wasm mem addr must be set before using this method"),
            lazy_pages::get_stack_end_wasm_addr(),
            lazy_pages::get_wasm_mem_size()
                .expect("Wasm mem size must be set before using this method") as usize,
            lazy_pages::get_released_pages().iter().copied(),
            protect,
        )
    }

    #[deprecated]
    fn save_page_lazy_info(page: u32, key: &[u8]) {
        lazy_pages::set_lazy_page_info(page.into(), key);
    }

    #[deprecated]
    #[version(2)]
    fn save_page_lazy_info(pages: Vec<u32>, prefix: Vec<u8>) {
        lazy_pages::append_lazy_pages_info(pages, prefix);
    }

    #[deprecated]
    fn init_lazy_pages() -> bool {
        lazy_pages::init(lazy_pages::LazyPagesVersion::Version1)
    }

    #[version(2)]
    fn init_lazy_pages() -> bool {
        lazy_pages::init(lazy_pages::LazyPagesVersion::Version2)
    }

    fn is_lazy_pages_enabled() -> bool {
        lazy_pages::is_enabled()
    }

    fn reset_lazy_pages_info() {
        lazy_pages::reset_context()
    }

    #[deprecated]
    fn set_wasm_mem_begin_addr(addr: u64) {
        lazy_pages::set_wasm_mem_begin_addr(addr);
    }

    #[version(2)]
    fn set_wasm_mem_begin_addr(addr: HostPointer) -> Result<(), RIError> {
        if addr % region::page::size() as u64 != 0 {
            return Err(RIError::WasmMemBufferNotAligned {
                addr: addr as u64,
                page_size: region::page::size() as u64,
            });
        }

        gear_lazy_pages::set_wasm_mem_begin_addr(addr);

        Ok(())
    }

    fn set_wasm_mem_size(size: u32) -> Result<(), RIError> {
        // TODO: remove this panic before release and make an error (issue #1147)
        assert_eq!(
            size as usize % region::page::size(),
            0,
            "Wasm memory buffer size is not aligned by host native page size"
        );

        lazy_pages::set_wasm_mem_size(size);
        Ok(())
    }

    fn initilize_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size: u32,
        stack_end_wasm_addr: Option<u32>,
        program_prefix: Vec<u8>,
    ) -> Result<(), RIError> {
        // TODO: remove this panic before release and make an error (issue #1147)
        lazy_pages::initilize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end_wasm_addr,
            program_prefix,
        )
        .expect("Cannot initilize lazy pages for current program");

        if let Some(addr) = wasm_mem_addr {
            unsafe { sys_mprotect_interval(addr, wasm_mem_size as usize, false, false, false) }
        } else {
            Ok(())
        }
    }

    // fn set_lazy_pages_addresses(stack_end_wasm_addr: WasmAddr) -> Result<(), RIError> {
    //     // TODO: remove this panic before release and make an error (issue #1147)
    //     assert_eq!(
    //         stack_end_wasm_addr as usize % WasmPageNumber::size(),
    //         0,
    //         "Stack end addr must be multiple of wasm page size"
    //     );
    //     lazy_pages::set_stack_end_wasm_addr(stack_end_wasm_addr);
    //     Ok(())
    // }

    fn set_program_prefix(prefix: Vec<u8>) {
        lazy_pages::set_program_prefix(prefix);
    }

    fn get_released_pages() -> Vec<u32> {
        lazy_pages::get_released_pages()
            .into_iter()
            .map(|p| p.0)
            .collect()
    }

    #[deprecated]
    fn get_released_page_old_data(page: u32) -> Vec<u8> {
        lazy_pages::get_released_page_data(page.into())
            .expect("Must have data for released page")
            .to_vec()
    }

    #[version(2)]
    #[deprecated]
    fn get_released_page_old_data(page: u32) -> Result<Vec<u8>, GetReleasedPageError> {
        lazy_pages::get_released_page_data(page.into())
            .ok_or(GetReleasedPageError)
            .map(|data| data.to_vec())
    }

    #[version(3)]
    #[deprecated]
    fn get_released_page_old_data(page: u32) -> Result<PageBuf, GetReleasedPageError> {
        lazy_pages::get_released_page_data(page.into()).ok_or(GetReleasedPageError)
    }

    #[version(4)]
    fn get_released_page_old_data(page: u32) -> Option<PageBuf> {
        lazy_pages::get_released_page_data(page.into())
    }

    #[deprecated]
    fn get_wasm_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::get_lazy_pages_numbers()
            .iter()
            .map(|p| p.0)
            .collect()
    }

    #[deprecated]
    fn get_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::get_lazy_pages_numbers()
            .iter()
            .map(|p| p.0)
            .collect()
    }
}
