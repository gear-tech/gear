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

use crate::{
    init_with_handler, mprotect, sys::ExceptionInfo, Error, LazyPagesVersion, UserSignalHandler,
};
use region::Protection;

#[test]
fn read_write_flag_works() {
    unsafe fn protect(access: bool) {
        let protection = if access {
            Protection::READ_WRITE
        } else {
            Protection::NONE
        };
        let page_size = region::page::size();
        let addr = MEM_ADDR;
        region::protect(addr, page_size, protection).unwrap();
    }

    unsafe fn invalid_write() {
        core::ptr::write_volatile(MEM_ADDR as *mut _, 123);
        protect(false);
    }

    unsafe fn invalid_read() {
        let _: u8 = core::ptr::read_volatile(MEM_ADDR);
        protect(false);
    }

    static mut COUNTER: u32 = 0;
    static mut MEM_ADDR: *const u8 = core::ptr::null_mut();

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
            let write_expected = COUNTER % 2 == 0;
            assert_eq!(info.is_write, Some(write_expected));

            protect(true);

            COUNTER += 1;

            Ok(())
        }
    }

    assert!(init_with_handler::<TestHandler>(
        LazyPagesVersion::Version1,
        Default::default()
    ));

    let page_size = region::page::size();
    let addr = region::alloc(page_size, Protection::NONE).unwrap();

    unsafe {
        MEM_ADDR = addr.as_ptr();

        invalid_write();
        invalid_read();
        invalid_write();
        invalid_read();
    }
}

#[test]
fn test_mprotect_pages() {
    use gear_core::memory::{PageNumber, PageU32Size, WasmPageNumber};

    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
            let mem = info.fault_addr as usize;
            let ps = region::page::size();
            let addr = region::page::floor(info.fault_addr);
            region::protect(addr, ps, region::Protection::READ_WRITE).unwrap();
            for p in 0..ps / PageNumber::size() as usize {
                *((mem + p * PageNumber::size() as usize) as *mut u8) = NEW_VALUE;
            }
            region::protect(addr, ps, region::Protection::READ).unwrap();

            Ok(())
        }
    }

    env_logger::init();

    assert!(init_with_handler::<TestHandler>(
        LazyPagesVersion::Version1,
        Default::default()
    ));

    let mut v = vec![0u8; 3 * WasmPageNumber::size() as usize];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = ((buff + WasmPageNumber::size() as usize) / WasmPageNumber::size() as usize)
        * WasmPageNumber::size() as usize;
    let mem_size = 2 * WasmPageNumber::size();

    // Gear pages in 2 wasm pages. Randomly choose pages, which will be protected,
    // but because macos with M1 has page size == 16kB, we should include all gear pages from 16kB interval.
    // This test can fail if page size is bigger than 16kB.
    let pages_protected =
        [0, 1, 2, 3, 16, 17, 18, 19, 20, 21, 22, 23].map(|p| PageNumber::new(p).unwrap());
    let pages_unprotected = [
        4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 26, 27, 28, 29, 30, 31,
    ]
    .map(|p| PageNumber::new(p).unwrap());

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for &p in pages_unprotected.iter().chain(pages_protected.iter()) {
            let addr = page_begin + p.offset() as usize + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect::mprotect_mem_interval_except_pages(
        page_begin,
        0,
        mem_size as usize,
        pages_unprotected.iter().copied(),
        false,
        false,
    )
    .expect("Must be correct");

    unsafe {
        for &p in pages_protected.iter() {
            let addr = page_begin + p.offset() as usize + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin + p.offset() as usize + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect::mprotect_mem_interval_except_pages(
        page_begin,
        0,
        mem_size as usize,
        pages_unprotected.iter().copied(),
        true,
        true,
    )
    .expect("Must be correct");

    // make the same for mprotect_pages

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for &p in pages_unprotected.iter().chain(pages_protected.iter()) {
            let addr = page_begin + p.offset() as usize + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect::mprotect_pages(page_begin, pages_protected.iter().copied(), false, false)
        .expect("Must be correct");

    unsafe {
        for &p in pages_protected.iter() {
            let addr = page_begin + p.offset() as usize + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin + p.offset() as usize + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect::mprotect_pages(page_begin, pages_protected.iter().copied(), true, true)
        .expect("Must be correct");
}
