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

use gear_core::memory::{HostPointer, PageBuf, PageNumber, WasmPageNumber};
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeMap, ops::Add};

mod sys;

thread_local! {
    /// NOTE: here we suppose, that each contract is executed in separate thread.
    /// Or may be in one thread but consequentially.

    /// Identify whether signal handler is set for current thread
    static LAZY_PAGES_ENABLED: RefCell<bool> = RefCell::new(false);
    /// Pointer to the begin of wasm memory buffer
    static WASM_MEM_BEGIN: RefCell<HostPointer> = RefCell::new(0);
    /// Key in storage for each lazy page
    static LAZY_PAGES_INFO: RefCell<BTreeMap<LazyPage, Vec<u8>>> = RefCell::new(BTreeMap::new());
    /// Page data, which has been in storage before current execution.
    /// For each lazy page, which has been accessed.
    static RELEASED_LAZY_PAGES: RefCell<BTreeMap<LazyPage, Option<PageBuf>>> = RefCell::new(BTreeMap::new());
}

pub fn save_lazy_pages_info(pages: Vec<u32>, prefix: Vec<u8>) {
    let pages_keys: BTreeMap<LazyPage, Vec<u8>> = pages.into_iter()
        .map(|p| {
            let mut key = Vec::with_capacity(prefix.len() + std::mem::size_of::<u32>());
            key.extend(prefix.clone());
            key.extend(p.to_le_bytes().to_vec());
            (LazyPage(p), key)
        })
        .collect();
    LAZY_PAGES_INFO.with(|lazy_pages_info| lazy_pages_info.borrow_mut().extend(pages_keys));
}

/// Returns vec of not-accessed wasm lazy pages
pub fn available_pages() -> Vec<LazyPage> {
    LAZY_PAGES_INFO.with(|lazy_pages_info| lazy_pages_info.borrow().keys().copied().collect())
}

/// Set current wasm memory begin addr
pub fn set_wasm_mem_begin_addr(wasm_mem_begin: HostPointer) {
    WASM_MEM_BEGIN.with(|x| *x.borrow_mut() = wasm_mem_begin);
}

/// Reset lazy pages info
pub fn reset_info() {
    LAZY_PAGES_INFO.with(|x| x.replace(BTreeMap::new()));
    RELEASED_LAZY_PAGES.with(|x| x.borrow_mut().clear());
    WASM_MEM_BEGIN.with(|x| *x.borrow_mut() = 0);
}

/// Returns vec of lazy pages which has been accessed
pub fn released_pages() -> Vec<LazyPage> {
    RELEASED_LAZY_PAGES.with(|x| x.borrow().keys().copied().collect())
}

/// Returns whether lazy pages env is enabled
pub fn is_enabled() -> bool {
    LAZY_PAGES_ENABLED.with(|x| *x.borrow())
}

/// Initialize lazy pages:
/// 1) checks whether lazy pages is supported in current environment
/// 2) set signals handler
///
/// # Safety
/// See [`sys::setup_signal_handler`]
pub unsafe fn init() -> bool {
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

    let ps = region::page::size();
    if ps > WasmPageNumber::size()
        || WasmPageNumber::size() % ps != 0
        || (ps > PageNumber::size() && ps % PageNumber::size() != 0)
        || (ps < PageNumber::size() && PageNumber::size() % ps != 0)
    {
        log::error!("Unsupported native pages size: {:#x}", ps);
        return false;
    }

    if let Err(err) = sys::setup_signal_handler() {
        log::error!("Failed to setup kernel signal handler: {}", err);
        return false;
    }

    log::debug!("Lazy pages are successfully enabled");
    LAZY_PAGES_ENABLED.with(|x| *x.borrow_mut() = true);

    true
}

#[derive(Debug, derive_more::Display)]
pub enum LazyPageError {
    #[display(fmt = "Exception is from unknown memory, page #{}", _0)]
    UnknownInfo(LazyPage),
    #[display(fmt = "Page #{} cannot be released twice", _0)]
    DoubleRelease(LazyPage),
}

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, derive_more::Display, derive_more::From,
)]
pub struct LazyPage(u32);

impl LazyPage {
    /// Save page key in storage
    pub fn set_info(self, key: &[u8]) {
        LAZY_PAGES_INFO
            .with(|lazy_pages_info| lazy_pages_info.borrow_mut().insert(self, key.to_vec()));
    }

    /// Returns data of released page which page has in storage before execution
    pub fn take_data(self) -> Option<PageBuf> {
        RELEASED_LAZY_PAGES.with(|x| x.borrow_mut().get_mut(&self)?.take())
    }

    fn release(self, page_buf: PageBuf) -> Result<(), LazyPageError> {
        RELEASED_LAZY_PAGES.with(move |released_pages| {
            // Restrict any page handling in signal handler more then one time.
            // If some page will be released twice it means, that this page has been added
            // to lazy pages more then one time during current execution.
            // This situation may cause problems with memory data update in storage.
            // For example: one page has no data in storage, but allocated for current program.
            // Let's make some action for it:
            // 1) Change data in page: Default data  ->  Data1
            // 2) Free page
            // 3) Alloc page, data will Data2 (may be equal Data1).
            // 4) After alloc we can set page as lazy, to identify wether page is changed after allocation.
            // This means that we can skip page update in storage in case it wasnt changed after allocation.
            // 5) Write some data in page but do not change it Data2 -> Data2.
            // During this step signal handler writes Data2 as data for released page.
            // 6) After execution we will have Data2 in page. And Data2 in released. So, nothing will be updated
            // in storage. But program may have some significant data for next execution - so we have a bug.
            // To avoid this we restrict double releasing.
            // You can also check another cases in test: memory_access_cases.
            let res = released_pages.borrow_mut().insert(self, Some(page_buf));
            if res.is_some() {
                Err(LazyPageError::DoubleRelease(self))
            } else {
                Ok(())
            }
        })
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl From<PageNumber> for LazyPage {
    fn from(PageNumber(page): PageNumber) -> Self {
        Self(page)
    }
}

impl Add<u32> for LazyPage {
    type Output = LazyPage;

    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0 + rhs)
    }
}
