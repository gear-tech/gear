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

#[test]
fn test_mprotect_pages() {
    use gear_core::memory::WasmPageNumber;

    const OLD_VALUE: u8 = 99;
    const NEW_VALUE: u8 = 100;

    unsafe fn test_handler(mem: usize) {
        let ps = region::page::size();
        let addr = ((mem / ps) * ps) as *mut ();
        region::protect(addr, ps, region::Protection::READ_WRITE).unwrap();
        for p in 0..ps / PageNumber::size() {
            *((mem + p * PageNumber::size()) as *mut u8) = NEW_VALUE;
        }
        region::protect(addr, ps, region::Protection::READ).unwrap();
    }

    #[cfg(unix)]
    unsafe {
        use libc::{c_void, siginfo_t};
        use nix::sys::signal;

        extern "C" fn handle_sigsegv(_: i32, info: *mut siginfo_t, _: *mut c_void) {
            unsafe {
                let mem = (*info).si_addr() as usize;
                test_handler(mem);
            }
        }

        let sig_handler = signal::SigHandler::SigAction(handle_sigsegv);
        let sig_action = signal::SigAction::new(
            sig_handler,
            signal::SaFlags::SA_SIGINFO,
            signal::SigSet::empty(),
        );
        signal::sigaction(signal::SIGSEGV, &sig_action).expect("Must be correct");
        signal::sigaction(signal::SIGBUS, &sig_action).expect("Must be correct");
    }

    #[cfg(windows)]
    unsafe {
        use winapi::{
            shared::ntdef::LONG,
            um::{
                errhandlingapi::SetUnhandledExceptionFilter,
                minwinbase::EXCEPTION_ACCESS_VIOLATION, winnt::EXCEPTION_POINTERS,
            },
            vc::excpt::EXCEPTION_CONTINUE_EXECUTION,
        };

        unsafe extern "system" fn exception_handler(
            exception_info: *mut EXCEPTION_POINTERS,
        ) -> LONG {
            let record = (*exception_info).ExceptionRecord;
            assert_eq!((*record).ExceptionCode, EXCEPTION_ACCESS_VIOLATION);
            assert_eq!((*record).NumberParameters, 2);

            let mem = (*record).ExceptionInformation[1] as usize;
            test_handler(mem);

            EXCEPTION_CONTINUE_EXECUTION
        }

        SetUnhandledExceptionFilter(Some(exception_handler));
    }

    let mut v = vec![0u8; 3 * WasmPageNumber::size()];
    let buff = v.as_mut_ptr() as usize;
    let page_begin = (((buff + WasmPageNumber::size()) / WasmPageNumber::size())
        * WasmPageNumber::size()) as u64;
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
            let addr = page_begin as usize + p.0 as usize * PageNumber::size() + 1;
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
            let addr = page_begin as usize + p.0 as usize * PageNumber::size() + 1;
            let x = *(addr as *mut u8);
            // value must be changed to `NEW_VALUE` in sig handler
            assert_eq!(x, NEW_VALUE);
        }

        for &p in pages_unprotected.iter() {
            let addr = page_begin as usize + p.0 as usize * PageNumber::size() + 1;
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
