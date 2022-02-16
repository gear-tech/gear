// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use gear_core::memory::PAGE_SIZE;

pub use sp_std::vec::Vec;

/// !!! Note: Will be expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    /// Apply mprotect syscall for given list of wasm pages.
    fn mprotect_wasm_pages(
        from_ptr: u64,
        pages_nums: &[u32],
        prot_read: bool,
        prot_write: bool,
        prot_exec: bool,
    ) {
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
            let addr = from_ptr as usize + *page as usize * PAGE_SIZE;
            let res = unsafe { libc::mprotect(addr as *mut libc::c_void, PAGE_SIZE, prot_mask) };
            assert!(
                res == 0,
                "Cannot set page protection for {:#x}: {}",
                addr,
                errno::errno()
            );
            log::trace!("mprotect wasm page: {:#x}, mask {:#x}", addr, prot_mask);
        }
    }

    fn save_page_lazy_info(wasm_page: u32, key: &[u8]) {
        gear_lazy_pages::save_page_lazy_info(wasm_page, key);
    }

    fn get_wasm_lazy_pages_numbers() -> Vec<u32> {
        gear_lazy_pages::get_wasm_lazy_pages_numbers()
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
        gear_lazy_pages::get_released_page_old_data(page)
    }

    fn print_hello() {
        println!("Hello from gear runtime interface!!!");
    }
}
