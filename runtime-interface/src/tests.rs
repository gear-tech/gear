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

use crate::*;
use gear_lazy_pages::{ExceptionInfo, LazyPagesVersion, UserSignalHandler};

#[test]
fn test_mprotect_pages() {
    use gear_core::memory::WasmPageNumber;

    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    struct TestHandler;

    impl UserSignalHandler for TestHandler {
        unsafe fn handle(info: ExceptionInfo) -> Result<(), lazy_pages::Error> {
            let mem = info.fault_addr as usize;
            let ps = region::page::size();
            let addr = region::page::floor(info.fault_addr);
            region::protect(addr, ps, region::Protection::READ_WRITE).unwrap();
            for p in 0..ps / PageNumber::size() {
                *((mem + p * PageNumber::size()) as *mut u8) = NEW_VALUE;
            }
            region::protect(addr, ps, region::Protection::READ).unwrap();

            Ok(())
        }
    }

    env_logger::init();

    assert!(lazy_pages::init::<TestHandler>(LazyPagesVersion::Version1));

    let mut v = vec![0u8; 3 * WasmPageNumber::size()];
    let buff = v.as_mut_ptr() as usize;
    let page_begin =
        ((buff + WasmPageNumber::size()) / WasmPageNumber::size()) * WasmPageNumber::size();
    let mem_size = 2 * WasmPageNumber::size();

    // Gear pages in 2 wasm pages. Randomly choose pages, which will be protected,
    // but because macos with M1 has page size == 16kB, we should include all gear pages from 16kB interval.
    // This test can fail if page size is bigger than 16kB.
    let pages_protected = [0, 1, 2, 3, 16, 17, 18, 19, 20, 21, 22, 23].map(PageNumber::from);
    let pages_unprotected = [
        4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 26, 27, 28, 29, 30, 31,
    ]
    .map(PageNumber::from);

    // Set `OLD_VALUE` as value for each first byte of gear pages
    unsafe {
        for &p in pages_unprotected.iter().chain(pages_protected.iter()) {
            let addr = page_begin + p.0 as usize * PageNumber::size() + 1;
            *(addr as *mut u8) = OLD_VALUE;
        }
    }

    mprotect_mem_interval_except_pages(
        page_begin,
        0,
        mem_size,
        pages_unprotected.iter().copied(),
        true,
    )
    .expect("Must be correct");

    unsafe {
        for &p in pages_protected.iter() {
            let addr = page_begin + p.0 as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin + p.0 as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must not be changed
            assert_eq!(x, OLD_VALUE);
        }
    }

    mprotect_mem_interval_except_pages(
        page_begin,
        0,
        mem_size,
        pages_unprotected.iter().copied(),
        false,
    )
    .expect("Must be correct");
}
