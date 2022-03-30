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

//! Module for memory and allocations context.

use alloc::collections::BTreeSet;
use codec::{Decode, Encode, HasCompact};
use scale_info::TypeInfo;

/// A WebAssembly page has a constant size of 64KiB.
pub const WASM_PAGE_SIZE: usize = 0x10000;

/// A gear page size.
/// TODO: comment
const GEAR_PAGE_SIZE: usize = 0x1000;

const GEAR_PAGES_IN_ONE_WASM: u32 = (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32;

/// Memory error.
#[derive(Clone, Debug)]
pub enum Error {
    /// Memory is over.
    ///
    /// All pages were previously allocated and there is nothing can be done.
    OutOfMemory,

    /// Allocation is in use.
    ///
    /// This is probably mis-use of the api (like dropping `Allocations` struct when some code is still runnig).
    AllocationsInUse,

    /// Specified page cannot be freed by the current program.
    ///
    /// It was allocated by another program.
    InvalidFree(WasmPageNumber),

    /// Out of bounds memory access
    MemoryAccessError,
}

/// Page buffer.
pub type PageBuf = [u8; GEAR_PAGE_SIZE];

/// Page number.
#[derive(
    Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq, Eq, PartialOrd, Ord, Hash
)]
pub struct PageNumber(u32);

impl PageNumber {
    /// Return raw 32-bit page address.
    pub fn raw(&self) -> u32 {
        self.0
    }

    /// Return page offset.
    pub fn offset(&self) -> usize {
        (self.0 as usize) * PageNumber::size()
    }

    /// Return page size in bytes.
    pub fn size() -> usize {
        GEAR_PAGE_SIZE
    }
}

impl core::ops::Add for PageNumber {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl core::ops::Sub for PageNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// Wasm page nuber
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, derive_more::From, TypeInfo)]
pub struct WasmPageNumber(pub u32);

impl WasmPageNumber {
    /// Return page offset.
    pub fn offset(&self) -> usize {
        (self.0 as usize) * WasmPageNumber::size()
    }

    /// TODO
    pub fn to_gear_pages(&self) -> PageNumber {
        PageNumber::from(self.0 * (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u32)
    }

    /// Return page size in bytes.
    pub const fn size() -> usize {
        WASM_PAGE_SIZE
    }
}

impl core::ops::Add for WasmPageNumber {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl core::ops::Sub for WasmPageNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// TODO
pub fn pages_set_to_wasm_pages_set(pages: &BTreeSet<PageNumber>) -> Result<BTreeSet<WasmPageNumber>, &'static str> {
    let mut wasm_pages = BTreeSet::<WasmPageNumber>::new();
    for page in pages {
        let wasm_page_num = WasmPageNumber(page.raw() / GEAR_PAGES_IN_ONE_WASM);
        if page.raw() % GEAR_PAGES_IN_ONE_WASM == 0 {
            wasm_pages.insert(wasm_page_num);
        } else {
            if !wasm_pages.contains(&wasm_page_num) {
                return Err("Has small page which is part of wasm page, but not all small pages from the wasm page");
            }
        }
    }
    Ok(wasm_pages)
}

/// TODO
pub fn wasm_pages_to_pages_set(wasm_pages: &BTreeSet<WasmPageNumber>) -> BTreeSet<PageNumber> {
    let mut pages = BTreeSet::<PageNumber>::new();
    for page in wasm_pages {
        pages.extend((page.0..page.0 + GEAR_PAGES_IN_ONE_WASM).map(|p| PageNumber::from(p)));
    }
    pages
}

/// Memory interface for the allocator.
pub trait Memory {
    /// Grow memory by number of pages.
    fn grow(&mut self, pages: WasmPageNumber) -> Result<PageNumber, Error>;

    /// Return current size of the memory.
    fn size(&self) -> WasmPageNumber;

    /// Set memory region at specific pointer.
    fn write(&mut self, offset: usize, buffer: &[u8]) -> Result<(), Error>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<(), Error>;

    /// Returns the byte length of this memory.
    fn data_size(&self) -> usize;

    /// Returns native addr of wasm memory buffer in wasm executor
    fn get_wasm_memory_begin_addr(&self) -> u64;
}

/// Pages allocations context for the running program.
pub struct AllocationsContext {
    /// Pages which has been in storage before execution
    init_allocations: BTreeSet<WasmPageNumber>,
    allocations: BTreeSet<WasmPageNumber>,
    max_pages: WasmPageNumber,
    static_pages: WasmPageNumber,
}

impl Clone for AllocationsContext {
    fn clone(&self) -> Self {
        Self {
            allocations: self.allocations.clone(),
            init_allocations: self.init_allocations.clone(),
            max_pages: self.max_pages,
            static_pages: self.static_pages,
        }
    }
}

impl AllocationsContext {
    /// New allocations context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    pub fn new(
        allocations: BTreeSet<WasmPageNumber>,
        static_pages: WasmPageNumber,
        max_pages: WasmPageNumber,
    ) -> Self {
        Self {
            init_allocations: allocations.clone(),
            allocations,
            max_pages,
            static_pages,
        }
    }

    /// Return `true` if the page is the initial page,
    /// it means that the page was already in the storage.
    pub fn is_init_page(&self, page: WasmPageNumber) -> bool {
        self.init_allocations.contains(&page)
    }

    /// Alloc specific number of pages for the currently running program.
    pub fn alloc(&mut self, pages: WasmPageNumber, mem: &mut dyn Memory) -> Result<WasmPageNumber, Error> {
        // silly allocator, brute-forces fist continuous sector
        let mut candidate = self.static_pages;
        let mut found = WasmPageNumber(0);

        while found < pages {
            if candidate + pages > self.max_pages {
                log::debug!(
                    "candidate: {:?}, pages: {:?}, max_pages: {:?}",
                    candidate,
                    pages,
                    self.max_pages
                );
                return Err(Error::OutOfMemory);
            }

            if self.allocations.contains(&(candidate + found).into()) {
                candidate = candidate + WasmPageNumber(1);
                found = WasmPageNumber(0);
                continue;
            }

            found = found + WasmPageNumber(1);
        }

        if candidate + found > mem.size() {
            let extra_grow = candidate + found - mem.size();
            mem.grow(extra_grow.into())?;
        }

        for page_num in candidate.0..(candidate + found).0 {
            self.allocations.insert(WasmPageNumber(page_num));
        }

        Ok(candidate)
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&mut self, page: WasmPageNumber) -> Result<(), Error> {
        if page < self.static_pages || page > self.max_pages {
            return Err(Error::InvalidFree(page));
        }
        self.allocations.remove(&page);

        Ok(())
    }

    /// Return reference to the allocation manager.
    pub fn allocations(&self) -> &BTreeSet<WasmPageNumber> {
        &self.allocations
    }
}

#[cfg(test)]
/// This module contains tests of PageNumber struct
mod tests {
    use super::PageNumber;

    #[test]
    /// Test that PageNumbers add up correctly
    fn page_number_addition() {
        let sum = PageNumber(100) + PageNumber(200);

        assert_eq!(sum, PageNumber(300));

        let sum = PageNumber(200) + PageNumber(100);

        assert_eq!(sum, PageNumber(300));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    /// Test that PageNumbers addition causes panic on overflow
    fn page_number_addition_with_overflow() {
        let _ = PageNumber(u32::MAX) + PageNumber(1);
    }

    #[test]
    /// Test that PageNumbers subtract correctly
    fn page_number_subtraction() {
        let subtraction = PageNumber(299) - PageNumber(199);

        assert_eq!(subtraction, PageNumber(100))
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    /// Test that PageNumbers subtraction causes panic on overflow
    fn page_number_subtraction_with_overflow() {
        let _ = PageNumber(1) - PageNumber(u32::MAX);
    }
}
