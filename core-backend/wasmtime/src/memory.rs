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

use alloc::{boxed::Box, collections::BTreeMap};
use core::any::Any;
use gear_core::memory::{Error, Memory, PageBuf, PageNumber};
use wasmtime::{Store, StoreContextMut};
use crate::env::StoreData;

/// Wrapper for wasmtime memory.
pub struct MemoryWrap<'a> {
    pub mem: wasmtime::Memory,
    pub store: StoreContextMut<'a, StoreData>,
}

/// Memory interface for the allocator.
impl<'a> Memory for MemoryWrap<'a> {
    fn grow(&mut self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.mem
            .grow(&mut self.store, pages.raw() as u64)
            .map(|offset| (offset as u32).into())
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        (self.mem.size(&self.store) as u32).into()
    }

    fn write(&mut self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.mem
            .write(&mut self.store, offset, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.mem
            .read(&self.store, offset, buffer)
            .expect("Memory out of bounds.")
    }

    fn data_size(&self) -> usize {
        self.mem.data_size(&self.store)
    }

    fn data_ptr(&self) -> *mut u8 {
        self.mem.data_ptr(&self.store)
    }

    fn get_wasm_memory_begin_addr(&self) -> usize {
        panic!("Not implemented");
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use gear_core::memory::MemoryContext;

//     fn new_test_memory(static_pages: u32, max_pages: u32) -> (MemoryContext, Box<dyn Memory>) {
//         use wasmtime::{Engine, Memory as WasmMemory, MemoryType};

//         let engine = Engine::default();
//         let mut store = Store::new(&engine, ());
//         wasmtime::StoreContextMut

//         let memory_ty = MemoryType::new(static_pages, Some(max_pages));
//         let mem = WasmMemory::new(store, memory_ty).expect("Memory creation failed");
//         let memory = MemoryWrap { mem, store: StoreContextMut::from_e };

//         (
//             MemoryContext::new(
//                 0.into(),
//                 Default::default(),
//                 static_pages.into(),
//                 max_pages.into(),
//             ),
//             Box::new(memory),
//         )
//     }

//     #[test]
//     fn smoky() {
//         let (mut ctx, mut mem) = new_test_memory(16, 256);

//         assert_eq!(ctx.alloc(16.into(), &mut mem).expect("allocation failed"), 16.into());

//         // there is a space for 14 more
//         for _ in 0..14 {
//             ctx.alloc(16.into(), &mut mem).expect("allocation failed");
//         }

//         // no more mem!
//         assert!(ctx.alloc(1.into(), &mut mem).is_err());

//         // but we free some
//         ctx.free(137.into()).expect("free failed");

//         // and now can allocate page that was freed
//         assert_eq!(ctx.alloc(1.into(), &mut mem).expect("allocation failed").raw(), 137);

//         // if we have 2 in a row we can allocate even 2
//         ctx.free(117.into()).expect("free failed");
//         ctx.free(118.into()).expect("free failed");

//         assert_eq!(ctx.alloc(2.into(), &mut mem).expect("allocation failed").raw(), 117);

//         // but if 2 are not in a row, bad luck
//         ctx.free(117.into()).expect("free failed");
//         ctx.free(158.into()).expect("free failed");

//         assert!(ctx.alloc(2.into(), &mut mem).is_err());
//     }
// }
