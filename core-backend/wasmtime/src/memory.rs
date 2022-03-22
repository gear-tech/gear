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

//! Wasmtime extensions for memory.

use crate::env::StoreData;
use gear_core::memory::{Error, Memory, PageNumber};
use wasmtime::StoreContextMut;

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

    fn get_wasm_memory_begin_addr(&self) -> usize {
        self.mem.data_ptr(&self.store) as usize
    }
}
