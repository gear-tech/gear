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

//! Module for memory and memory context.

use crate::program::ProgramId;
use alloc::collections::BTreeMap;
use alloc::{boxed::Box, collections::BTreeSet};
use codec::{Decode, Encode};
use core::any::Any;

/// A WebAssembly page has a constant size of 65,536 bytes, i.e., 64KiB.
pub const PAGE_SIZE: usize = 65536;

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

    /// Specified page is occupied.
    PageOccupied(PageNumber),

    /// Specified page cannot be freed by the current program.
    ///
    /// It was allocated by another program.
    InvalidFree(PageNumber),

    /// Out of bounds memory access
    MemoryAccessError,
}

/// Page buffer.
pub type PageBuf = [u8; PAGE_SIZE];

/// Page number.
#[derive(
    Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct PageNumber(u32);

impl PageNumber {
    /// Return raw 32-bit page address.
    pub fn raw(&self) -> u32 {
        self.0
    }

    /// Return page offset.
    pub fn offset(&self) -> usize {
        (self.0 as usize) * PAGE_SIZE
    }

    /// Return page size in bytes.
    pub fn size() -> usize {
        PAGE_SIZE
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

/// Memory interface for the allocator.
pub trait Memory: Any {
    /// Grow memory by number of pages.
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error>;

    /// Return current size of the memory.
    fn size(&self) -> PageNumber;

    /// Set memory region at specific pointer.
    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: usize, buffer: &mut [u8]);

    /// Returns the byte length of this memory.
    fn data_size(&self) -> usize;

    /// Returns the base pointer, in the host’s address space, that the memory is located at.
    fn data_ptr(&self) -> *mut u8;

    /// Set memory pages from PageBuf map, grow if possible.
    fn set_pages(&self, pages: &BTreeMap<PageNumber, Box<PageBuf>>) -> Result<(), Error> {
        for (num, buf) in pages {
            let s = self.size() - 1.into();
            if s < *num {
                self.grow(*num - s)?;
            }
            self.write(num.offset(), &buf[..])?;
        }
        Ok(())
    }

    /// Clone this memory.
    fn clone(&self) -> Box<dyn Memory>;

    /// Downcast to exact memory type
    fn as_any(&self) -> &dyn Any;
}

/// Memory context for the running program.
pub struct MemoryContext {
    program_id: ProgramId,
    memory: Box<dyn Memory>,
    /// Pages which has been in storage.
    init_allocations: BTreeSet<PageNumber>,
    allocations: BTreeSet<PageNumber>,
    max_pages: PageNumber,
    static_pages: PageNumber,
}

impl Clone for MemoryContext {
    fn clone(&self) -> Self {
        Self {
            program_id: self.program_id,
            memory: self.memory.clone(),
            allocations: self.allocations.clone(),
            init_allocations: self.init_allocations.clone(),
            max_pages: self.max_pages,
            static_pages: self.static_pages,
        }
    }
}

impl Clone for Box<dyn Memory> {
    fn clone(self: &Box<dyn Memory>) -> Box<dyn Memory> {
        Memory::clone(&**self)
    }
}

impl MemoryContext {
    /// New memory context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    pub fn new(
        program_id: ProgramId,
        memory: Box<dyn Memory>,
        allocations: BTreeSet<PageNumber>,
        static_pages: PageNumber,
        max_pages: PageNumber,
    ) -> Self {
        Self {
            program_id,
            memory,
            init_allocations: allocations.clone(),
            allocations,
            max_pages,
            static_pages,
        }
    }

    /// Return `true` if the page is the initial page,
    /// it means that the page was already in the storage.
    pub fn is_init_page(&self, page: PageNumber) -> bool {
        self.init_allocations.contains(&page)
    }

    /// Return currently used program id.
    pub fn program_id(&self) -> ProgramId {
        self.program_id
    }

    /// Alloc specific number of pages for the currently running program.
    pub fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, Error> {
        // silly allocator, brute-forces fist continuous sector
        let mut candidate = self.static_pages.raw();
        let mut found = 0u32;

        while found < pages.raw() {
            if candidate + pages.raw() > self.max_pages.raw() {
                log::debug!(
                    "candidate: {}, pages: {}, max_pages: {}",
                    candidate,
                    pages.raw(),
                    self.max_pages.raw()
                );
                return Err(Error::OutOfMemory);
            }

            if self.allocations.contains(&(candidate + found).into()) {
                candidate += 1;
                found = 0;
                continue;
            }

            found += 1;
        }

        if candidate + found > self.memory.size().raw() {
            let extra_grow = candidate + found - self.memory.size().raw();
            self.memory.grow(extra_grow.into())?;
        }

        for page_num in candidate..candidate + found {
            self.allocations.insert(page_num.into());
        }

        Ok(candidate.into())
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&mut self, page: PageNumber) -> Result<(), Error> {
        if page < self.static_pages || page > self.max_pages {
            return Err(Error::InvalidFree(page));
        }
        self.allocations.remove(&page);

        Ok(())
    }

    /// Return reference to the allocation manager.
    pub fn allocations(&self) -> &BTreeSet<PageNumber> {
        &self.allocations
    }

    /// Return reference to the memory blob.
    pub fn memory(&self) -> &dyn Memory {
        &*self.memory
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
