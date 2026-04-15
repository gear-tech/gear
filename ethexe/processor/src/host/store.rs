// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use core::ops::Range;
use sp_allocator::{AllocationStats, FreeingBumpHeapAllocator};
use wasmtime::{AsContextMut, Caller, Memory, Table};

fn checked_range(offset: usize, len: usize, max: usize) -> Option<Range<usize>> {
    let end = offset.checked_add(len)?;
    (end <= max).then(|| offset..end)
}

pub(crate) struct HostState {
    pub(crate) allocator: Option<FreeingBumpHeapAllocator>,
    pub(crate) panic_message: Option<String>,
}

impl HostState {
    pub(crate) fn new(allocator: FreeingBumpHeapAllocator) -> Self {
        Self {
            allocator: Some(allocator),
            panic_message: None,
        }
    }

    pub(crate) fn allocation_stats(&self) -> AllocationStats {
        self.allocator
            .as_ref()
            .expect("allocator is always restored after host calls; qed")
            .stats()
    }
}

#[derive(Default)]
pub(crate) struct StoreData {
    pub(crate) host_state: Option<HostState>,
    pub(crate) memory: Option<Memory>,
    pub(crate) table: Option<Table>,
}

impl StoreData {
    pub(crate) fn host_state_mut(&mut self) -> Option<&mut HostState> {
        self.host_state.as_mut()
    }

    pub(crate) fn memory(&self) -> Memory {
        self.memory
            .expect("memory is initialized before host calls; qed")
    }
}

pub(crate) struct MemoryWrapper<'a, C>(&'a Memory, &'a mut C);

impl<'a, C> From<(&'a Memory, &'a mut C)> for MemoryWrapper<'a, C> {
    fn from((memory, ctx): (&'a Memory, &'a mut C)) -> Self {
        Self(memory, ctx)
    }
}

impl<C: AsContextMut> sp_allocator::Memory for MemoryWrapper<'_, C> {
    fn with_access<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
        run(self.0.data(&self.1))
    }

    fn with_access_mut<R>(&mut self, run: impl FnOnce(&mut [u8]) -> R) -> R {
        run(self.0.data_mut(&mut self.1))
    }

    fn grow(&mut self, additional: u32) -> std::result::Result<(), ()> {
        self.0
            .grow(&mut self.1, additional as u64)
            .map_err(|err| {
                log::error!("Failed to grow memory by {additional} pages: {err}");
            })
            .map(drop)
    }

    fn pages(&self) -> u32 {
        self.0.size(&self.1) as u32
    }

    fn max_pages(&self) -> Option<u32> {
        self.0.ty(&self.1).maximum().map(|pages| pages as u32)
    }
}

pub(crate) fn write_memory_from(
    mut ctx: impl AsContextMut<Data = StoreData>,
    address: u32,
    data: &[u8],
) -> Result<(), String> {
    let memory = ctx.as_context().data().memory();
    let memory = memory.data_mut(&mut ctx);

    let range = checked_range(address as usize, data.len(), memory.len())
        .ok_or_else(|| String::from("memory write is out of bounds"))?;
    memory[range].copy_from_slice(data);
    Ok(())
}

fn host_state_mut<'a>(caller: &'a mut Caller<'_, StoreData>) -> &'a mut HostState {
    caller
        .data_mut()
        .host_state_mut()
        .expect("host state is initialized before wasm calls; qed")
}

pub(crate) fn allocate_memory(
    caller: &mut Caller<'_, StoreData>,
    size: u32,
) -> Result<u32, String> {
    let mut allocator = host_state_mut(caller)
        .allocator
        .take()
        .expect("allocator is available during wasm calls; qed");

    let memory = caller.data().memory();
    let res = allocator
        .allocate(
            &mut MemoryWrapper::from((&memory, &mut caller.as_context_mut())),
            size,
        )
        .map(u32::from)
        .map_err(|err| err.to_string());

    host_state_mut(caller).allocator = Some(allocator);

    res
}

pub(crate) fn deallocate_memory(
    caller: &mut Caller<'_, StoreData>,
    ptr: u32,
) -> Result<(), String> {
    let mut allocator = host_state_mut(caller)
        .allocator
        .take()
        .expect("allocator is available during wasm calls; qed");

    let memory = caller.data().memory();
    let ptr = ptr.into();
    let res = allocator
        .deallocate(
            &mut MemoryWrapper::from((&memory, &mut caller.as_context_mut())),
            ptr,
        )
        .map_err(|err| err.to_string());

    host_state_mut(caller).allocator = Some(allocator);

    res
}
