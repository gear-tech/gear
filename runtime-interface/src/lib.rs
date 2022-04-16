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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use gear_core::memory::PageNumber;

pub use sp_std::{result::Result, vec::Vec};

#[derive(Debug, Encode, Decode)]
pub enum MprotectError {
    PageError,
    OsError,
}

#[derive(Debug, Encode, Decode)]
pub struct GetReleasedPageError;

/// TODO: deprecated remove before release
#[cfg(feature = "std")]
#[cfg(unix)]
unsafe fn sys_mprotect_wasm_pages(
    from_ptr: u64,
    pages_nums: &[u32],
    prot_read: bool,
    prot_write: bool,
    prot_exec: bool,
) -> Result<(), MprotectError> {
    use gear_core::memory::WasmPageNumber;

    let mut prot_mask = libc::PROT_NONE;
    if prot_read {
        prot_mask |= libc::PROT_READ;
    }
    if prot_write {
        prot_mask |= libc::PROT_WRITE;
    }
    if prot_exec {
        prot_mask |= libc::PROT_EXEC;
    }
    for page in pages_nums {
        let addr = from_ptr as usize + *page as usize * WasmPageNumber::size();
        let res = libc::mprotect(addr as *mut libc::c_void, WasmPageNumber::size(), prot_mask);
        if res != 0 {
            log::error!(
                "Cannot set page protection for {:#x}: {}",
                addr,
                errno::errno()
            );
            return Err(MprotectError::PageError);
        }
        log::trace!("mprotect wasm page: {:#x}, mask {:#x}", addr, prot_mask);
    }
    Ok(())
}

#[cfg(feature = "std")]
#[cfg(not(unix))]
unsafe fn sys_mprotect_wasm_pages(
    _from_ptr: u64,
    _pages_nums: &[u32],
    _prot_read: bool,
    _prot_write: bool,
    _prot_exec: bool,
) -> Result<(), MprotectError> {
    log::error!("unsupported OS for pages protectection");
    Err(MprotectError::OsError)
}

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection argumetns.
#[cfg(feature = "std")]
#[cfg(unix)]
unsafe fn sys_mprotect_interval(
    addr: u64,
    size: usize,
    prot_read: bool,
    prot_write: bool,
    prot_exec: bool,
) -> Result<(), MprotectError> {
    if size == 0 || size % page_size::get() != 0 {
        return Err(MprotectError::PageError);
    }
    let mut prot_mask = libc::PROT_NONE;
    if prot_read {
        prot_mask |= libc::PROT_READ;
    }
    if prot_write {
        prot_mask |= libc::PROT_WRITE;
    }
    if prot_exec {
        prot_mask |= libc::PROT_EXEC;
    }
    let res = libc::mprotect(addr as *mut libc::c_void, size, prot_mask);
    if res != 0 {
        log::error!(
            "Cannot set page protection for {:#x}: {}",
            addr,
            errno::errno()
        );
        return Err(MprotectError::PageError);
    }
    log::trace!("mprotect native page: {:#x}, mask {:#x}", addr, prot_mask);
    Ok(())
}

#[cfg(feature = "std")]
#[cfg(not(unix))]
unsafe fn sys_mprotect_interval(
    _addr: u64,
    _size: usize,
    _prot_read: bool,
    _prot_write: bool,
    _prot_exec: bool,
) -> Result<(), MprotectError> {
    log::error!("unsupported OS for pages protectection");
    Err(MprotectError::OsError)
}

/// !!! Note: Will be expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    /// TODO: deprecated remove before release
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

    /// TODO: deprecated remove before release
    /// Apply mprotect syscall for given list of wasm pages.
    #[version(2)]
    fn mprotect_wasm_pages(
        from_ptr: u64,
        pages_nums: &[u32],
        prot_read: bool,
        prot_write: bool,
        prot_exec: bool,
    ) -> Result<(), MprotectError> {
        unsafe { sys_mprotect_wasm_pages(from_ptr, pages_nums, prot_read, prot_write, prot_exec) }
    }

    /// Mprotect all lazy pages.
    /// If `protect` argument is true then restrict all accesses to page,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), MprotectError> {
        let mut prev_page = 0u32;
        let mut size_in_pages = 0u32;
        let mut lazy_pages = gear_lazy_pages::get_lazy_pages_numbers();

        if let Some(page) = lazy_pages.last() {
            // This case is impossible in real live, so just returns err and does nothing.
            if *page == u32::MAX || *page == u32::MAX - 1 {
                return Err(MprotectError::PageError);
            }
        }

        // We add this page num to lazy pages in order to be able correctly
        // finish lazy pages handling in loop. This last page won't be
        // handled.
        lazy_pages.push(u32::MAX);

        // Collects continuous intervals of memory from lazy pages to protect them.
        for page in lazy_pages {
            if prev_page + 1 == page {
                size_in_pages += 1;
            } else {
                if size_in_pages != 0 {
                    let addr = wasm_mem_addr
                        + ((prev_page + 1 - size_in_pages) as usize * PageNumber::size()) as u64;
                    let size = size_in_pages as usize * PageNumber::size();
                    if protect {
                        unsafe { sys_mprotect_interval(addr, size, false, false, false)? };
                    } else {
                        unsafe { sys_mprotect_interval(addr, size, true, true, false)? };
                    }
                }
                size_in_pages = 1;
            }
            prev_page = page;
        }
        Ok(())
    }

    fn save_page_lazy_info(page: u32, key: &[u8]) {
        gear_lazy_pages::save_page_lazy_info(page, key);
    }

    /// TODO: deprecated remove before release
    fn get_wasm_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::get_lazy_pages_numbers()
    }

    fn get_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::get_lazy_pages_numbers()
    }

    fn init_lazy_pages() -> bool {
        unsafe { gear_lazy_pages::init_lazy_pages() }
    }

    fn reset_lazy_pages_info() {
        gear_lazy_pages::reset_lazy_pages_info()
    }

    fn set_wasm_mem_begin_addr(addr: u64) {
        gear_lazy_pages::set_wasm_mem_begin_addr(addr as usize);
    }

    fn get_released_pages() -> Vec<u32> {
        gear_lazy_pages::get_released_pages()
    }

    fn get_released_page_old_data(page: u32) -> Vec<u8> {
        gear_lazy_pages::get_released_page_old_data(page).expect("Must have data for released page")
    }

    #[version(2)]
    fn get_released_page_old_data(page: u32) -> Result<Vec<u8>, GetReleasedPageError> {
        gear_lazy_pages::get_released_page_old_data(page).map_err(|_| GetReleasedPageError)
    }

    fn print_hello() {
        println!("Hello from gear runtime interface!!!");
    }
}
