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

pub use sp_std::{result::Result, vec::Vec};

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
unsafe fn sys_mprotect_interval(
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

#[cfg(feature = "std")]
fn mprotect_pages_slice(
    mem_addr: HostPointer,
    pages: &[gear_lazy_pages::LazyPage],
    protect: bool,
) -> Result<(), RIError> {
    let mprotect = |start: gear_lazy_pages::LazyPage, count, protect: bool| unsafe {
        let addr = mem_addr + (start.as_u32() as usize * PageNumber::size()) as HostPointer;
        let size = count as usize * PageNumber::size();
        sys_mprotect_interval(addr, size, !protect, !protect, false)
    };

    // Collects continuous intervals of memory from lazy pages to protect them.
    let mut start = if let Some(&start) = pages.first() {
        start
    } else {
        return Ok(());
    };

    let mut count = 1;
    for &page in pages.iter().skip(1) {
        if start + count == page {
            count = count.saturating_add(1);
        } else {
            mprotect(start, count, protect)?;
            start = page as _;
            count = 1;
        }
    }
    mprotect(start, count, protect)
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    // TODO: deprecated, remove before release
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

    // TODO: deprecated, remove before release.
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

    // TODO: deprecated, remove before release
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), MprotectError> {
        let lazy_pages = gear_lazy_pages::available_pages();
        mprotect_pages_slice(wasm_mem_addr, &lazy_pages, protect).map_err(|err| match err {
            RIError::UnsupportedOS => MprotectError::OsError,
            _ => MprotectError::PageError,
        })
    }

    /// Mprotect all lazy pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    #[version(2)]
    fn mprotect_lazy_pages(wasm_mem_addr: u64, protect: bool) -> Result<(), RIError> {
        let lazy_pages = gear_lazy_pages::available_pages();
        mprotect_pages_slice(wasm_mem_addr, &lazy_pages, protect)
    }

    fn save_page_lazy_info(page: u32, key: &[u8]) {
        gear_lazy_pages::LazyPage::from(page).set_info(key);
    }

    // TODO: deprecated, remove before release
    fn get_wasm_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::available_pages()
            .into_iter()
            .map(gear_lazy_pages::LazyPage::as_u32)
            .collect()
    }

    fn get_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::available_pages()
            .into_iter()
            .map(gear_lazy_pages::LazyPage::as_u32)
            .collect()
    }

    fn init_lazy_pages() -> bool {
        unsafe { gear_lazy_pages::init() }
    }

    fn is_lazy_pages_enabled() -> bool {
        gear_lazy_pages::is_enabled()
    }

    fn reset_lazy_pages_info() {
        gear_lazy_pages::reset_info()
    }

    // TODO: deprecated, remove before release
    fn set_wasm_mem_begin_addr(addr: u64) {
        gear_lazy_pages::set_wasm_mem_begin_addr(addr);
    }

    // TODO: deprecated, remove before release
    #[version(2)]
    fn set_wasm_mem_begin_addr(addr: HostPointer) -> Result<(), RIError> {
        #[cfg(not(unix))]
        {
            let _addr = addr;
            Err(RIError::UnsupportedOS)
        }

        #[cfg(unix)]
        {
            if addr % region::page::size() as u64 != 0 {
                return Err(RIError::WasmMemBufferNotAligned {
                    addr: addr as u64,
                    page_size: region::page::size() as u64,
                });
            }

            gear_lazy_pages::set_wasm_mem_begin_addr(addr);

            Ok(())
        }
    }

    #[version(3)]
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

    fn get_released_pages() -> Vec<u32> {
        gear_lazy_pages::released_pages()
            .into_iter()
            .map(gear_lazy_pages::LazyPage::as_u32)
            .collect()
    }

    // TODO: deprecated, remove before release
    fn get_released_page_old_data(page: u32) -> Vec<u8> {
        gear_lazy_pages::LazyPage::from(page)
            .take_data()
            .expect("Must have data for released page")
            .to_vec()
    }

    // TODO: deprecated, remove before release
    #[version(2)]
    fn get_released_page_old_data(page: u32) -> Result<Vec<u8>, GetReleasedPageError> {
        gear_lazy_pages::LazyPage::from(page)
            .take_data()
            .ok_or(GetReleasedPageError)
            .map(|data| data.to_vec())
    }

    // TODO: deprecated, remove before release
    #[version(3)]
    fn get_released_page_old_data(page: u32) -> Result<PageBuf, GetReleasedPageError> {
        gear_lazy_pages::LazyPage::from(page)
            .take_data()
            .ok_or(GetReleasedPageError)
    }

    #[version(4)]
    fn get_released_page_old_data(page: u32) -> Option<PageBuf> {
        gear_lazy_pages::LazyPage::from(page).take_data()
    }

    fn print_hello() {
        println!("Hello from gear runtime interface!!!");
    }
}

#[cfg(feature = "std")]
#[cfg(unix)]
#[test]
unsafe fn test_mprotect_pages_vec() {
    use gear_core::memory::WasmPageNumber;
    use libc::{c_void, siginfo_t};
    use nix::sys::signal;

    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    extern "C" fn handle_sigsegv(_: i32, info: *mut siginfo_t, _: *mut c_void) {
        unsafe {
            let mem = (*info).si_addr() as usize;
            let ps = region::page::size();
            let addr = ((mem / ps) * ps) as *mut c_void;
            region::protect(addr, ps, region::Protection::WRITE).unwrap();
            for p in 0..ps / PageNumber::size() {
                *((mem + p * PageNumber::size()) as *mut u8) = NEW_VALUE;
            }
            region::protect(addr, ps, region::Protection::READ).unwrap();
        }
    }

    let mut v = vec![0u8; 3 * WasmPageNumber::size()];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = (((buff + WasmPageNumber::size()) / WasmPageNumber::size())
        * WasmPageNumber::size()) as u64;

    mprotect_pages_slice(page_begin + 1, &[0, 1, 2, 3], true)
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

    mprotect_pages_slice(page_begin, &pages_to_protect, true).expect("Must be correct");

    unsafe {
        let sig_handler = signal::SigHandler::SigAction(handle_sigsegv);
        let sig_action = signal::SigAction::new(
            sig_handler,
            signal::SaFlags::SA_SIGINFO,
            signal::SigSet::empty(),
        );
        signal::sigaction(signal::SIGSEGV, &sig_action).expect("Must be correct");
        signal::sigaction(signal::SIGBUS, &sig_action).expect("Must be correct");

        for &p in pages_to_protect.iter() {
            let addr = page_begin as usize + p as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin as usize + p as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect_pages_slice(page_begin, &pages_to_protect, false).expect("Must be correct");
}
