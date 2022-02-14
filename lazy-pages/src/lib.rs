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

//! Lazy pages support.
//! In runtime data for contract wasm memory pages can be loaded in lazy manner.
//! All pages, which is supposed to be lazy, must be mprotected before contract execution.
//! During execution data from storage is loaded for all pages, which has been accesed.
//! See also [handle_sigsegv].

use std::{cell::RefCell, collections::BTreeMap};

use gear_core::memory::{PageNumber, PAGE_SIZE};
use libc::{c_void, siginfo_t};
use nix::sys::signal;

thread_local! {
    /// NOTE: here we suppose, that each contract is executed in separate thread.
    /// Or may be in one thread but consequentially.

    /// Identify whether signal handler is set
    static LAZY_PAGES_ENABLED: RefCell<bool> = RefCell::new(false);
    /// Pointer to the begin of wasm memory buffer
    static WASM_MEM_BEGIN: RefCell<usize> = RefCell::new(0);
    /// Key in storage for each lazy page
    static LAZY_PAGES_INFO: RefCell<BTreeMap<u32, Vec<u8>>> = RefCell::new(BTreeMap::new());
    /// Page data, which has been in storage before current execution.
    /// For each lazy page, which has been accessed.
    static RELEASED_LAZY_PAGES: RefCell<BTreeMap<u32, Vec<u8>>> = RefCell::new(BTreeMap::new());
}

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
    let (wasm_page, wasm_page_native_addr) = unsafe {
        log::debug!(target: "gear_node::sig_handler", "Interrupted, sig-info = {:?}", *info);
        let mem = (*info).si_addr();
        let native_page = (mem as usize / page_size::get()) * page_size::get();
        let wasm_mem_begin = WASM_MEM_BEGIN.with(|x| *x.borrow());
        assert!(wasm_mem_begin != 0, "Wasm memory begin addr is not set");
        // TODO: we need to do something here. May be throw it to old sig handler.
        assert!(wasm_mem_begin <= native_page, "Unknown sisegv/sigbus");
        let wasm_page: PageNumber = (((native_page - wasm_mem_begin) / PAGE_SIZE) as u32).into();
        let wasm_page_native_addr = wasm_mem_begin + wasm_page.offset();
        log::debug!(target: "gear_node::sig_handler", "mem={:#x} native_page={:#x} wasm_page={} wasm_page_addr={:#x}", mem as usize, native_page, wasm_page.raw(), wasm_page_native_addr);
        (wasm_page, wasm_page_native_addr)
    };

    let page_info = LAZY_PAGES_INFO.with(|info| info.borrow_mut().remove(&wasm_page.raw()));
    if page_info.is_none() {
        // TODO: we need to do something here. May be throw it to old sig handler.
        panic!("sigsegv/sigbus from unknown memory");
    }

    let res = unsafe {
        libc::mprotect(
            wasm_page_native_addr as *mut libc::c_void,
            PAGE_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
        )
    };
    assert!(
        res == 0,
        "Cannot remove page protection, unexpected os behavior: {}",
        errno::errno()
    );

    let page_as_slice =
        unsafe { std::slice::from_raw_parts_mut(wasm_page_native_addr as *mut u8, PAGE_SIZE) };
    let hash_key_in_storage = page_info.unwrap();
    let res = sp_io::storage::read(&hash_key_in_storage, page_as_slice, 0);
    assert!(res.is_some(), "Wasm page must have data in storage");
    assert!(
        res.unwrap() as usize == PAGE_SIZE,
        "Page data must contain {} bytes, actually has {}",
        PAGE_SIZE,
        res.unwrap()
    );

    let res = RELEASED_LAZY_PAGES.with(|rpages| {
        rpages
            .borrow_mut()
            .insert(wasm_page.raw(), page_as_slice.to_vec())
    });
    assert!(res.is_none(), "Any page cannot be released twice");

    log::debug!(target: "gear_node::sig_handler", "Finish signal handling");
}

/// Save page key in storage
pub fn save_page_lazy_info(wasm_page: u32, key: &[u8]) {
    LAZY_PAGES_INFO
        .with(|lazy_pages_info| lazy_pages_info.borrow_mut().insert(wasm_page, key.to_vec()));
}

/// Returns vec of not-accessed wasm lazy pages
pub fn get_wasm_lazy_pages_numbers() -> sp_std::vec::Vec<u32> {
    LAZY_PAGES_INFO.with(|lazy_pages_info| lazy_pages_info.borrow().iter().map(|x| *x.0).collect())
}

/// Set current wasm memory begin addr
pub fn set_wasm_mem_begin_addr(wasm_mem_begin: usize) {
    WASM_MEM_BEGIN.with(|x| *x.borrow_mut() = wasm_mem_begin);
}

/// Reset lazy pages info
pub fn reset_lazy_pages_info() {
    LAZY_PAGES_INFO.with(|x| x.replace(BTreeMap::new()));
    RELEASED_LAZY_PAGES.with(|x| x.replace(BTreeMap::new()));
    WASM_MEM_BEGIN.with(|x| x.replace(0));
}

/// Returns vec of lazy pages which has been accessed
pub fn get_released_pages() -> sp_std::vec::Vec<u32> {
    RELEASED_LAZY_PAGES.with(|x| x.borrow().iter().map(|x| *x.0).collect())
}

/// Returns page data which page has in storage before execution
pub fn get_released_page_old_data(page: u32) -> sp_std::vec::Vec<u8> {
    RELEASED_LAZY_PAGES.with(|x| x.borrow().get(&page).expect("Must have this page").to_vec())
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

    assert!(LAZY_PAGES_INFO.with(|x| x.borrow().is_empty()));
    assert!(WASM_MEM_BEGIN.with(|x| *x.borrow() == 0));
    assert!(RELEASED_LAZY_PAGES.with(|x| x.borrow().is_empty()));

    if page_size::get() > PAGE_SIZE || PAGE_SIZE % page_size::get() != 0 {
        log::debug!("Unsupported native pages size: {:#x}", page_size::get());
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
            log::error!(
                "Cannot set sigsegv handler: {}",
                errno::Errno(err_no as i32)
            );
            return false;
        }
    } else if cfg!(target_os = "macos") {
        let res = signal::sigaction(signal::SIGBUS, &sig_action);
        if let Err(err_no) = res {
            log::error!("Cannot set sigbus handler: {}", errno::Errno(err_no as i32));
            return false;
        }
    } else {
        log::debug!("Lazy pages doesn't support this OS");
        return false;
    }

    log::debug!("Lazy pages are successfully enabled");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    true
}
