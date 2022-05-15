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

//! Lazy pages support in unix.

use gear_core::memory::{PageBuf, PageNumber, WasmPageNumber};
use libc::{c_void, siginfo_t};
use nix::sys::signal;

use crate::{LAZY_PAGES_ENABLED, LAZY_PAGES_INFO, RELEASED_LAZY_PAGES, WASM_MEM_BEGIN};

/// Sigsegv (or sigbus for macos) handler.
/// Before contract execution some pages from wasm memory buffer are protected,
/// and cannot be accessed anyhow. When wasm executer tries to access one of these pages,
/// OS emits sigsegv or sigbus. We handle the signal in this function.
/// Using OS signal info, we identify memory location and wasm page.
/// We remove read and write protections for page,
/// then we load wasm page data from storage to wasm page memory location.
/// Also we save page data to [RELEASED_LAZY_PAGES] in order to identify later
/// whether page is changed after execution.
/// After signal handler is done, OS returns execution to the same machine
/// instruction, which cause signal. Now memory which this instruction accesses
/// is not protected and with correct data.
extern "C" fn handle_sigsegv(_x: i32, info: *mut siginfo_t, _z: *mut c_void) {
    // In this function we use panics in check instead of err return, because it's signal handler.

    let native_ps = page_size::get();
    let gear_ps = PageNumber::size();

    let (gear_page, gear_pages_num, unprot_addr) = unsafe {
        log::debug!("Interrupted, sig-info = {:?}", *info);

        let mem = (*info).si_addr();
        let native_page = (mem as usize / native_ps) * native_ps;
        let wasm_mem_begin = WASM_MEM_BEGIN.with(|x| *x.borrow());

        assert!(wasm_mem_begin != 0, "Wasm memory begin addr is not set");
        assert!(
            wasm_mem_begin <= native_page,
            "sisegv/sigbus from unknown memory"
        );

        // First gear page which must be unprotected
        let gear_page = PageNumber::from(((native_page - wasm_mem_begin) / gear_ps) as u32);

        let res = if native_ps > gear_ps {
            assert!(native_ps % gear_ps == 0);
            (gear_page, native_ps / gear_ps, native_page)
        } else {
            assert!(gear_ps % native_ps == 0);
            (gear_page, 1usize, wasm_mem_begin + gear_page.offset())
        };

        let accessed_page = PageNumber::from(((mem as usize - wasm_mem_begin) / gear_ps) as u32);
        log::debug!(
            "mem={:?} accessed={:?},{:?} pages={:?} page_native_addr={:#x}",
            mem,
            accessed_page,
            accessed_page.to_wasm_page(),
            res.0 .0..res.0 .0 + res.1 as u32,
            res.2
        );

        res
    };

    let unprot_size = gear_pages_num * gear_ps;

    let res = unsafe {
        libc::mprotect(
            unprot_addr as *mut libc::c_void,
            unprot_size,
            libc::PROT_READ | libc::PROT_WRITE,
        )
    };
    assert!(
        res == 0,
        "Cannot remove page protection, unexpected os behavior: {}",
        errno::errno()
    );

    for idx in 0..gear_pages_num as u32 {
        let page = gear_page.0 + idx;

        let hash_key_in_storage = LAZY_PAGES_INFO
            .with(|info| info.borrow_mut().remove(&page))
            .expect("sigsegv/sigbus from unknown memory");

        let buffer_as_slice = unsafe {
            let ptr = (unprot_addr as *mut u8).add(idx as usize * gear_ps);
            std::slice::from_raw_parts_mut(ptr, gear_ps)
        };

        let res = sp_io::storage::read(&hash_key_in_storage, buffer_as_slice, 0);

        if res.is_none() {
            log::trace!("Page {:?} has no data in storage, so just save current page data to released pages", page);
        } else {
            log::trace!("Page {:?} has data in storage, so set this data for page and save it in released pages", page);
        }

        assert!(
            res.is_none() || res.unwrap() as usize == PageNumber::size(),
            "Page data must contain {} bytes, actually has {}",
            PageNumber::size(),
            res.unwrap()
        );

        RELEASED_LAZY_PAGES.with(|released_pages| {
            let page_buf = PageBuf::new_from_vec(buffer_as_slice.to_vec())
                .expect("Cannot panic because we create slice with PageBuf size");
            let res = released_pages.borrow_mut().insert(page, Some(page_buf));
            assert!(res.is_none(), "Any page cannot be released twice");
        });
    }
}

/// Initialize lazy pages:
/// 1) checks whether lazy pages is supported in current environment
/// 2) set signals handler
///
/// # Safety
/// use OS specific functions
pub unsafe fn init_lazy_pages() -> bool {
    if LAZY_PAGES_ENABLED.with(|x| *x.borrow()) {
        log::trace!("Lazy-pages has been already enabled");
        return true;
    }

    if !LAZY_PAGES_INFO.with(|x| x.borrow().is_empty()) {
        log::error!("Lazy pages info must be empty before initialization");
        return false;
    }
    if !WASM_MEM_BEGIN.with(|x| *x.borrow() == 0) {
        log::error!("Wasm mem begin must be 0 before initialization");
        return false;
    }
    if !RELEASED_LAZY_PAGES.with(|x| x.borrow().is_empty()) {
        log::error!("Released lazy pages must be empty before initialization");
        return false;
    }

    let ps = page_size::get();
    if ps > WasmPageNumber::size()
        || WasmPageNumber::size() % ps != 0
        || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
        || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
    {
        log::debug!("Unsupported native pages size: {:#x}", ps);
        return false;
    }

    let handler = signal::SigHandler::SigAction(handle_sigsegv);
    let sig_action = signal::SigAction::new(
        handler,
        signal::SaFlags::SA_SIGINFO,
        signal::SigSet::empty(),
    );

    if cfg!(target_os = "linux") {
        let res = signal::sigaction(signal::SIGSEGV, &sig_action);
        if let Err(err_no) = res {
            log::debug!(
                target: "essential",
                "Cannot set sigsegv handler: {}",
                errno::Errno(err_no as i32),
            );
            return false;
        }
    } else if cfg!(target_os = "macos") {
        let res = signal::sigaction(signal::SIGBUS, &sig_action);
        if let Err(err_no) = res {
            log::debug!(
                target: "essential",
                "Cannot set sigbus handler: {}",
                errno::Errno(err_no as i32),
            );
            return false;
        }
    } else {
        log::debug!("Lazy pages are not supported on this OS");
        return false;
    }

    log::debug!("Lazy pages are successfully enabled");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    true
}
