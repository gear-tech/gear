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
    common::Error,
    init_with_handler, mprotect,
    pages::{GearPageNumber, PageDynSize},
    signal::ExceptionInfo,
    LazyPagesVersion, UserSignalHandler,
};
use gear_core::memory::{GearPage, PageU32Size, WasmPage};
use region::Protection;

fn handler_tester<F: FnOnce()>(f: F) {
    crate::reset_init_flag();
    f();
}

#[test]
fn test_with_different_handlers() {
    handler_tester(read_write_flag_works);
    handler_tester(test_mprotect_pages);
}

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

    init_with_handler::<TestHandler>(
        LazyPagesVersion::Version1,
        vec![WasmPage::size(), GearPage::size()],
        vec!["".to_string(); 2],
        Default::default(),
    )
    .unwrap();

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

fn test_mprotect_pages() {
    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    let page_size = 0x4000;
    let new_page = |p: u32| GearPageNumber::new(p, &page_size).unwrap();
    let offset = |p: GearPageNumber| p.offset(&page_size) as usize;

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
            let mem = info.fault_addr as usize;
            let addr = region::page::floor(info.fault_addr);
            region::protect(
                addr,
                GearPage::size() as usize,
                region::Protection::READ_WRITE,
            )
            .unwrap();
            *(mem as *mut u8) = NEW_VALUE;
            region::protect(addr, GearPage::size() as usize, region::Protection::READ).unwrap();

            Ok(())
        }
    }

    env_logger::init();

    init_with_handler::<TestHandler>(
        LazyPagesVersion::Version1,
        vec![WasmPage::size(), GearPage::size()],
        vec!["".to_string(); 2],
        Default::default(),
    )
    .unwrap();

    let mut v = vec![0u8; 3 * WasmPage::size() as usize];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = ((buff + WasmPage::size() as usize) / WasmPage::size() as usize)
        * WasmPage::size() as usize;
    let mem_size = 2 * WasmPage::size();

    // Randomly choose pages, which will be protected.
    let pages_protected = [0, 4, 5].map(new_page);
    let pages_unprotected = [1, 2, 3, 6, 7].map(new_page);

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for &p in pages_unprotected.iter().chain(pages_protected.iter()) {
            let addr = page_begin + offset(p) + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect::mprotect_mem_interval_except_pages(
        page_begin,
        0,
        mem_size as usize,
        pages_unprotected.iter().copied(),
        &GearPage::size(),
        false,
        false,
    )
    .expect("Must be correct");

    unsafe {
        for &p in pages_protected.iter() {
            let addr = page_begin + offset(p) + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin + offset(p) + 1;
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
        &page_size,
        true,
        true,
    )
    .expect("Must be correct");

    // make the same for mprotect_pages

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for &p in pages_unprotected.iter().chain(pages_protected.iter()) {
            let addr = page_begin + offset(p) + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect::mprotect_pages(
        page_begin,
        pages_protected.iter().copied(),
        &page_size,
        false,
        false,
    )
    .expect("Must be correct");

    unsafe {
        for &p in pages_protected.iter() {
            let addr = page_begin + offset(p) + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin + offset(p) + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect::mprotect_pages(
        page_begin,
        pages_protected.iter().copied(),
        &page_size,
        true,
        true,
    )
    .expect("Must be correct");
}
