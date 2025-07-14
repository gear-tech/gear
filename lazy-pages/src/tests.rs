// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
    LazyPagesStorage, LazyPagesVersion, UserSignalHandler,
    common::Error,
    init_with_handler, mprotect,
    pages::{GearPage, tests::PageSizeManager},
    signal::ExceptionInfo,
};
use gear_core::str::LimitedStr;
use gear_lazy_pages_common::LazyPagesInitContext;
use numerated::tree::IntervalsTree;
use region::Protection;

const GEAR_PAGE_SIZE: usize = 0x4000;
const WASM_PAGE_SIZE: usize = 0x10000;

#[derive(Debug)]
struct NoopStorage;

impl LazyPagesStorage for NoopStorage {
    fn page_exists(&self, _key: &[u8]) -> bool {
        unreachable!()
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!()
    }
}

fn init_ctx() -> LazyPagesInitContext {
    LazyPagesInitContext {
        page_sizes: vec![WASM_PAGE_SIZE as u32, GEAR_PAGE_SIZE as u32],
        global_names: vec![LimitedStr::from_small_str("gear_gas")],
        pages_storage_prefix: Default::default(),
    }
}

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
        let addr = unsafe { MEM_ADDR };
        unsafe { region::protect(addr, page_size, protection) }.unwrap();
    }

    unsafe fn invalid_write() {
        unsafe {
            core::ptr::write_volatile(MEM_ADDR as *mut _, 123);
            protect(false);
        }
    }

    unsafe fn invalid_read() {
        unsafe {
            let _: u8 = core::ptr::read_volatile(MEM_ADDR);
            protect(false);
        }
    }

    static mut COUNTER: u32 = 0;
    static mut MEM_ADDR: *const u8 = core::ptr::null_mut();

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
            let write_expected = unsafe { COUNTER }.is_multiple_of(2);
            assert_eq!(info.is_write, Some(write_expected));

            unsafe {
                protect(true);

                COUNTER += 1;
            }

            Ok(())
        }
    }

    init_with_handler::<TestHandler, _>(LazyPagesVersion::Version1, init_ctx(), NoopStorage)
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

    let ctx = PageSizeManager([WASM_PAGE_SIZE as u32, GEAR_PAGE_SIZE as u32]);
    let new_page = |p: u32| GearPage::new(&ctx, p).unwrap();
    let offset = |p: GearPage| p.offset(&ctx) as usize;

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), Error> {
            let mem = info.fault_addr as usize;
            let addr = region::page::floor(info.fault_addr);
            unsafe {
                region::protect(addr, GEAR_PAGE_SIZE, region::Protection::READ_WRITE).unwrap();
                *(mem as *mut u8) = NEW_VALUE;
                region::protect(addr, GEAR_PAGE_SIZE, region::Protection::READ).unwrap();
            }

            Ok(())
        }
    }

    tracing_subscriber::fmt::init();

    init_with_handler::<TestHandler, _>(LazyPagesVersion::Version1, init_ctx(), NoopStorage)
        .unwrap();

    let mut v = vec![0u8; 3 * WASM_PAGE_SIZE];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = ((buff + WASM_PAGE_SIZE) / WASM_PAGE_SIZE) * WASM_PAGE_SIZE;

    let pages: IntervalsTree<_> = (0..(2 * WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32)
        .map(new_page)
        .collect();

    // Randomly choose pages, which is going to be protected.
    let pages_protected: IntervalsTree<_> = [0, 4, 5].map(new_page).into_iter().collect();
    let pages_unprotected: IntervalsTree<_> = pages.difference(&pages).collect();
    assert!(pages.end() >= pages_protected.end());

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for p in pages.points_iter() {
            let addr = page_begin + offset(p) + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect::mprotect_pages(page_begin, pages_protected.iter(), &ctx, false, false)
        .expect("Must be correct");

    unsafe {
        for p in pages_protected.points_iter() {
            let addr = page_begin + offset(p) + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for p in pages_unprotected.points_iter() {
            let addr = page_begin + offset(p) + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect::mprotect_pages(page_begin, pages_protected.iter(), &ctx, true, true)
        .expect("Must be correct");
}
