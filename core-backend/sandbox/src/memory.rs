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

//! sp-sandbox extensions for memory and memory context.

use alloc::boxed::Box;
use core::any::Any;

use gear_core::memory::{Error, Memory, PageNumber};

/// Wrapper for sp_sandbox::Memory.
pub struct MemoryWrap(sp_sandbox::Memory);

impl MemoryWrap {
    /// Wrap sp_sandbox::Memory for Memory trait.
    pub fn new(mem: sp_sandbox::Memory) -> Self {
        MemoryWrap(mem)
    }
}

/// Memory interface for the allocator.
impl Memory for MemoryWrap {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.0
            .grow(pages.raw())
            .map(|prev| prev.into())
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        self.0.size().into()
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.0
            .set(offset as u32, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.0
            .get(offset as u32, buffer)
            .expect("Memory out of bounds.");
    }

    fn data_size(&self) -> usize {
        (self.0.size() * 65536) as usize
    }

    fn data_ptr(&self) -> *mut u8 {
        todo!()
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
    }

    fn as_any(&self) -> &dyn Any {
        &self.0
    }
}

impl Clone for MemoryWrap {
    fn clone(self: &MemoryWrap) -> Self {
        MemoryWrap(self.0.clone())
    }
}

// can't be tested outside the node runtime
#[cfg(test)]
mod tests {
    use super::*;
    use gear_core::memory::MemoryContext;

    fn new_test_memory(static_pages: u32, max_pages: u32) -> MemoryContext {
        use sp_sandbox::Memory as WasmMemory;

        let memory = MemoryWrap::new(
            WasmMemory::new(static_pages, Some(max_pages)).expect("Memory creation failed"),
        );

        MemoryContext::new(
            0.into(),
            Box::new(memory),
            Default::default(),
            static_pages.into(),
            max_pages.into(),
        )
    }

    #[test]
    fn smoky() {
        let mut mem = new_test_memory(16, 256);

        assert_eq!(mem.alloc(16.into()).expect("allocation failed"), 16.into());

        // there is a space for 14 more
        for _ in 0..14 {
            mem.alloc(16.into()).expect("allocation failed");
        }

        // no more mem!
        assert!(mem.alloc(1.into()).is_err());

        // but we free some
        mem.free(137.into()).expect("free failed");

        // and now can allocate page that was freed
        assert_eq!(mem.alloc(1.into()).expect("allocation failed").raw(), 137);

        // if we have 2 in a row we can allocate even 2
        mem.free(117.into()).expect("free failed");
        mem.free(118.into()).expect("free failed");

        assert_eq!(mem.alloc(2.into()).expect("allocation failed").raw(), 117);

        // but if 2 are not in a row, bad luck
        mem.free(117.into()).expect("free failed");
        mem.free(158.into()).expect("free failed");

        assert!(mem.alloc(2.into()).is_err());
    }
}
