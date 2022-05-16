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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum MprotectError {
    #[display(fmt = "Page error")]
    PageError,
    #[display(fmt = "OS error")]
    OsError,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[display(fmt = "Failed to get released page")]
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
            "Cannot set page protection for addr={:#x} size={:#x} mask={:#x}: {}",
            addr,
            size,
            prot_mask,
            errno::errno()
        );
        return Err(MprotectError::PageError);
    }
    log::trace!(
        "mprotect native mem interval: {:#x}, size: {:#x}, mask {:#x}",
        addr,
        size,
        prot_mask
    );
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

#[cfg(feature = "std")]
fn mprotect_pages_vec(mem_addr: u64, pages: &[u32], protect: bool) -> Result<(), MprotectError> {
    if pages.is_empty() {
        return Ok(());
    }

    let mprotect = |start, count, protect: bool| unsafe {
        let addr = mem_addr + (start * PageNumber::size()) as u64;
        let size = count * PageNumber::size();
        sys_mprotect_interval(addr, size, !protect, !protect, false)
    };

    // Collects continuous intervals of memory from lazy pages to protect them.
    let mut start = *pages
        .first()
        .expect("We checked that `pages` are not empty") as usize;
    let mut count = 1;
    for page in pages.iter().skip(1) {
        if start + count == *page as usize {
            count = count.saturating_add(1);
        } else {
            mprotect(start, count, protect)?;
            start = *page as _;
            count = 1;
        }
    }
    mprotect(start, count, protect)
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
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), MprotectError> {
        let lazy_pages = gear_lazy_pages::get_lazy_pages_numbers();
        mprotect_pages_vec(wasm_mem_addr, &lazy_pages, protect)
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

    fn is_lazy_pages_enabled() -> bool {
        gear_lazy_pages::is_lazy_pages_enabled()
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

#[cfg(feature = "std")]
#[cfg(unix)]
#[test]
fn test_mprotect_pages_vec() {
    use gear_core::memory::WasmPageNumber;
    use libc::{c_void, siginfo_t};
    use nix::sys::signal;

    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    extern "C" fn handle_sigsegv(_: i32, info: *mut siginfo_t, _: *mut c_void) {
        unsafe {
            let mem = (*info).si_addr() as usize;
            let ps = page_size::get();
            let addr = ((mem / ps) * ps) as *mut c_void;
            if libc::mprotect(addr, ps, libc::PROT_WRITE) != 0 {
                panic!("Cannot set protection: {}", errno::errno());
            }
            for p in 0..ps / PageNumber::size() {
                *((mem + p * PageNumber::size()) as *mut u8) = NEW_VALUE;
            }
            if libc::mprotect(addr, ps, libc::PROT_READ) != 0 {
                panic!("Cannot set protection: {}", errno::errno());
            }
        }
    }

    let mut v = vec![0u8; 3 * WasmPageNumber::size()];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = (((buff + WasmPageNumber::size()) / WasmPageNumber::size())
        * WasmPageNumber::size()) as u64;

    mprotect_pages_vec(page_begin + 1, &[0, 1, 2, 3], true)
        .expect_err("Must fail because page_begin + 1 is not aligned addr");

    let pages_to_protect = [0, 1, 2, 3, 16, 17, 18, 19, 20, 21, 22, 23];
    let pages_unprotected = [
        4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 26, 27, 28, 29, 30, 31,
    ];

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for p in pages_to_protect.iter().chain(pages_unprotected.iter()) {
            let addr = page_begin as usize + *p as usize * PageNumber::size() + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect_pages_vec(page_begin, &pages_to_protect, true).expect("Must be correct");

    unsafe {
        let sig_handler = signal::SigHandler::SigAction(handle_sigsegv);
        let sig_action = signal::SigAction::new(
            sig_handler,
            signal::SaFlags::SA_SIGINFO,
            signal::SigSet::empty(),
        );
        signal::sigaction(signal::SIGSEGV, &sig_action).expect("Must be correct");
        signal::sigaction(signal::SIGBUS, &sig_action).expect("Must be correct");

        for p in pages_to_protect.iter() {
            let addr = page_begin as usize + *p as usize * PageNumber::size() + 1;
            let _ = *(addr as *mut u8);
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }
        for p in pages_unprotected.iter() {
            let addr = page_begin as usize + *p as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect_pages_vec(page_begin, &pages_to_protect, false).expect("Must be correct");
}
