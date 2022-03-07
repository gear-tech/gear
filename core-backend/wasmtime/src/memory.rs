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

//! Wasmtime extensions for memory and memory context.

use crate::env::LaterStore;
use alloc::boxed::Box;
use core::any::Any;
use gear_core::memory::{Error, Memory, PageNumber};

/// Wrapper for wasmtime memory.
pub struct MemoryWrap {
    pub mem: wasmtime::Memory,
    pub store: LaterStore<()>,
}

impl MemoryWrap {
    /// Wrap wasmtime memory for Memory trait.
    pub fn new(mem: wasmtime::Memory, store: &LaterStore<()>) -> Self {
        Self {
            mem,
            store: store.clone(),
        }
    }
}

/// Memory interface for the allocator.
impl Memory for MemoryWrap {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.mem
            .grow(self.store.clone().get_mut_ref(), pages.raw() as u64)
            .map(|offset| (offset as u32).into())
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        (self.mem.size(self.store.clone().get_mut_ref()) as u32).into()
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.mem
            .write(self.store.clone().get_mut_ref(), offset, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.mem
            .read(self.store.clone().get_mut_ref(), offset, buffer)
            .expect("Memory out of bounds.")
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
    }

    fn data_size(&self) -> usize {
        self.mem.data_size(self.store.clone().get_mut_ref())
    }

    fn data_ptr(&self) -> *mut u8 {
        self.mem.data_ptr(self.store.clone().get_mut_ref())
    }

    fn as_any(&self) -> &dyn Any {
        &self.mem
    }

    fn get_wasm_memory_begin_addr(&self) -> usize {
        panic!("Not implemented");
    }
}

impl Clone for MemoryWrap {
    fn clone(self: &MemoryWrap) -> Self {
        Self {
            mem: self.mem,
            store: self.store.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_core::memory::MemoryContext;

    fn new_test_memory(static_pages: u32, max_pages: u32) -> MemoryContext {
        use wasmtime::{Engine, Memory as WasmMemory, MemoryType};

        let engine = Engine::default();
        let mut store = LaterStore::<()>::new(&engine);

        let memory_ty = MemoryType::new(static_pages, Some(max_pages));
        let mem = WasmMemory::new(store.get_mut_ref(), memory_ty).expect("Memory creation failed");
        let memory = MemoryWrap::new(mem, &store);

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
