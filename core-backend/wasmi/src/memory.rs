// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! wasmi extensions for memory.

use gear_backend_common::state::HostState;
use gear_core::{
    env::Externalities,
    memory::{HostPointer, Memory, MemoryError},
    pages::{PageNumber, PageU32Size, WasmPage},
};
use wasmi::{core::memory_units::Pages, Memory as WasmiMemory, Store, StoreContextMut};

pub(crate) struct MemoryWrapRef<'a, Ext: Externalities + 'static> {
    pub memory: WasmiMemory,
    pub store: StoreContextMut<'a, HostState<Ext, WasmiMemory>>,
}

impl<'a, Ext: Externalities + 'static> Memory for MemoryWrapRef<'a, Ext> {
    type GrowError = wasmi::errors::MemoryError;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory
            .grow(&mut self.store, Pages(pages.raw() as usize))
            .map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.current_pages(&self.store).0 as u32)
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.data_mut(&mut self.store).as_mut().as_mut_ptr() as HostPointer
    }
}

/// Wrapper for [`wasmi::Memory`].
pub struct MemoryWrap<Ext: Externalities + 'static> {
    pub(crate) memory: WasmiMemory,
    pub(crate) store: Store<HostState<Ext, WasmiMemory>>,
}

impl<Ext: Externalities + 'static> MemoryWrap<Ext> {
    /// Wrap [`wasmi::Memory`] for Memory trait.
    pub(crate) fn new(memory: WasmiMemory, store: Store<HostState<Ext, WasmiMemory>>) -> Self {
        MemoryWrap { memory, store }
    }
    pub(crate) fn into_store(self) -> Store<HostState<Ext, WasmiMemory>> {
        self.store
    }
}

/// Memory interface for the allocator.
impl<Ext: Externalities + 'static> Memory for MemoryWrap<Ext> {
    type GrowError = wasmi::errors::MemoryError;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory
            .grow(&mut self.store, Pages(pages.raw() as usize))
            .map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.current_pages(&self.store).0 as u32)
            .expect("Unexpected backend behavior: wasm memory is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset as usize, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.data_mut(&mut self.store).as_mut().as_mut_ptr() as HostPointer
    }
}
