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
use gear_core::memory::{HostPointer, PageBuf, WasmPageNumber};
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

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

#[cfg(test)]
mod tests;

// TODO: issue #1147. Make this error for mprotection and for internal use only.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum RIError {
    #[display(fmt = "Cannot mprotect interval {:#x?}, mask = {}", interval, mask)]
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
    addr: usize,
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
            interval: addr as u64..=addr as u64 + size as u64,
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
    mem_addr: usize,
    start_offset: usize,
    mem_size: usize,
    except_pages: impl Iterator<Item = PageNumber>,
    protect: bool,
) -> Result<(), RIError> {
    let mprotect = |start, end| {
        let addr = mem_addr + start;
        let size = end - start;
        unsafe { sys_mprotect_interval(addr, size, !protect, !protect, false) }
    };

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
            lazy_pages::get_wasm_mem_addr()
                .expect("Wasm mem addr must be set before using this method"),
            lazy_pages::get_stack_end_wasm_addr() as usize,
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
        lazy_pages::set_wasm_mem_begin_addr(addr as usize);
    }

    #[version(2)]
    fn set_wasm_mem_begin_addr(addr: HostPointer) -> Result<(), RIError> {
        if addr % region::page::size() as u64 != 0 {
            return Err(RIError::WasmMemBufferNotAligned {
                addr: addr as u64,
                page_size: region::page::size() as u64,
            });
        }

        gear_lazy_pages::set_wasm_mem_begin_addr(addr as usize);

        Ok(())
    }

    #[deprecated]
    fn set_wasm_mem_size(size: u32) -> Result<(), RIError> {
        assert_eq!(
            size as usize % region::page::size(),
            0,
            "Wasm memory buffer size is not aligned by host native page size"
        );

        lazy_pages::set_wasm_mem_size(size);
        Ok(())
    }

    #[version(2)]
    fn set_wasm_mem_size(size_in_wasm_pages: u32) {
        let size = WasmPageNumber(size_in_wasm_pages);
        let size_in_bytes =
            u32::try_from(size.offset()).expect("Wasm memory size is bigger then u32::MAX bytes");
        lazy_pages::set_wasm_mem_size(size_in_bytes);
    }

    fn initialize_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size: u32,
        stack_end_page: Option<u32>,
        program_prefix: Vec<u8>,
    ) -> Result<(), RIError> {
        let wasm_mem_size = wasm_mem_size.into();
        let stack_end_page = stack_end_page.map(Into::into);

        let wasm_mem_addr = wasm_mem_addr
            .map(|addr| usize::try_from(addr).expect("Cannot cast wasm mem addr to `usize`"));
        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end_page,
            program_prefix,
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");

        if let Some(addr) = wasm_mem_addr {
            let stack_end = stack_end_page.map(|p| p.offset()).unwrap_or(0);
            let size = wasm_mem_size.offset();
            let except_pages = std::iter::empty::<PageNumber>();
            mprotect_mem_interval_except_pages(addr, stack_end, size, except_pages, true)
        } else {
            Ok(())
        }
    }

    fn set_program_prefix(prefix: Vec<u8>) {
        lazy_pages::set_program_prefix(prefix);
    }

    fn get_released_pages() -> Vec<u32> {
        lazy_pages::get_released_pages_patch()
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
