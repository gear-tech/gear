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
use gear_core::{
    env::Ext,
    memory::{Error, HostPointer, Memory, PageNumber, WasmPageNumber},
};
use wasmtime::{AsContext, AsContextMut, Store, StoreContextMut};

pub struct MemoryWrapExternal<E: Ext> {
    pub mem: wasmtime::Memory,
    pub store: Store<StoreData<E>>,
}

impl<E: Ext> Memory for MemoryWrapExternal<E> {
    fn grow(&mut self, pages: WasmPageNumber) -> Result<PageNumber, Error> {
        self.mem
            .grow(&mut self.store, pages.0 as u64)
            .map(|offset| (offset as u32).into())
            .map_err(|_| Error::OutOfBounds)
    }

    fn size(&self) -> WasmPageNumber {
        (self.mem.size(&self.store) as u32).into()
    }

    fn write(&mut self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.mem
            .write(&mut self.store, offset, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<(), Error> {
        self.mem
            .read(&self.store, offset, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn data_size(&self) -> usize {
        self.mem.data_size(&self.store)
    }

    unsafe fn get_buffer_host_addr_unsafe(&self) -> HostPointer {
        self.mem.data_ptr(&self.store) as HostPointer
    }
}

pub fn grow<T: Ext>(
    ctx: impl AsContextMut<Data = StoreData<T>>,
    mem: wasmtime::Memory,
    pages: WasmPageNumber,
) -> Result<PageNumber, Error> {
    mem.grow(ctx, pages.0 as u64)
        .map(|offset| (offset as u32).into())
        .map_err(|_| Error::OutOfBounds)
}

pub fn size<T: Ext>(
    ctx: impl AsContext<Data = StoreData<T>>,
    mem: wasmtime::Memory,
) -> WasmPageNumber {
    (mem.size(ctx) as u32).into()
}

pub fn write<T: Ext>(
    ctx: impl AsContextMut<Data = StoreData<T>>,
    mem: wasmtime::Memory,
    offset: usize,
    buffer: &[u8],
) -> Result<(), Error> {
    mem.write(ctx, offset, buffer)
        .map_err(|_| Error::MemoryAccessError)
}

pub fn read<T: Ext>(
    ctx: impl AsContext<Data = StoreData<T>>,
    mem: wasmtime::Memory,
    offset: usize,
    buffer: &mut [u8],
) -> Result<(), Error> {
    mem.read(ctx, offset, buffer)
        .map_err(|_| Error::MemoryAccessError)
}

fn data_size<T: Ext>(ctx: impl AsContext<Data = StoreData<T>>, mem: wasmtime::Memory) -> usize {
    mem.data_size(ctx)
}

unsafe fn get_buffer_host_addr_unsafe<T: Ext>(
    ctx: impl AsContext<Data = StoreData<T>>,
    mem: wasmtime::Memory,
) -> HostPointer {
    mem.data_ptr(ctx) as HostPointer
}
