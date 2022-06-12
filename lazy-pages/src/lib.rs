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

//! Lazy pages support.
//! In runtime data for contract wasm memory pages can be loaded in lazy manner.
//! All pages, which is supposed to be lazy, must be mprotected before contract execution.
//! During execution data from storage is loaded for all pages, which has been accesed.
//! See also `handle_sigsegv`.

use gear_core::memory::{HostPointer, PageBuf};
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeMap};

#[cfg(unix)]
#[path = "unix.rs"]
mod sys;

#[cfg(not(unix))]
#[path = "unsupported.rs"]
mod sys;

thread_local! {
    /// NOTE: here we suppose, that each contract is executed in separate thread.
    /// Or may be in one thread but consequentially.

    /// Identify whether signal handler is set for current thread
    static LAZY_PAGES_ENABLED: RefCell<bool> = RefCell::new(false);
    /// Pointer to the begin of wasm memory buffer
    static WASM_MEM_BEGIN: RefCell<HostPointer> = RefCell::new(0);
    /// Key in storage for each lazy page
    static LAZY_PAGES_INFO: RefCell<BTreeMap<u32, Vec<u8>>> = RefCell::new(BTreeMap::new());
    /// Page data, which has been in storage before current execution.
    /// For each lazy page, which has been accessed.
    static RELEASED_LAZY_PAGES: RefCell<BTreeMap<u32, Option<PageBuf>>> = RefCell::new(BTreeMap::new());
}

/// Save page key in storage
pub fn save_page_lazy_info(page: u32, key: &[u8]) {
    LAZY_PAGES_INFO.with(|lazy_pages_info| lazy_pages_info.borrow_mut().insert(page, key.to_vec()));
}

/// Returns vec of not-accessed wasm lazy pages
pub fn get_lazy_pages_numbers() -> Vec<u32> {
    LAZY_PAGES_INFO.with(|lazy_pages_info| lazy_pages_info.borrow().iter().map(|x| *x.0).collect())
}

/// Set current wasm memory begin addr
pub fn set_wasm_mem_begin_addr(wasm_mem_begin: HostPointer) {
    WASM_MEM_BEGIN.with(|x| *x.borrow_mut() = wasm_mem_begin);
}

/// Reset lazy pages info
pub fn reset_lazy_pages_info() {
    LAZY_PAGES_INFO.with(|x| x.replace(BTreeMap::new()));
    RELEASED_LAZY_PAGES.with(|x| x.replace(BTreeMap::new()));
    WASM_MEM_BEGIN.with(|x| x.replace(0));
}

/// Returns vec of lazy pages which has been accessed
pub fn get_released_pages() -> Vec<u32> {
    RELEASED_LAZY_PAGES.with(|x| x.borrow().iter().map(|x| *x.0).collect())
}

/// Returns whether lazy pages env is enabled
pub fn is_lazy_pages_enabled() -> bool {
    LAZY_PAGES_ENABLED.with(|x| *x.borrow())
}

/// Returns page data which page has in storage before execution
pub fn get_released_page_old_data(page: u32) -> Option<PageBuf> {
    RELEASED_LAZY_PAGES.with(|x| x.borrow_mut().get_mut(&page)?.take())
}

pub use sys::init_lazy_pages;
