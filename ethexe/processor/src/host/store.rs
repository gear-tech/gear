// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use core::ops::Range;
use ethexe_common::injected::Promise;
use ethexe_runtime_common::unpack_i64_to_u32;
use parity_scale_codec::Decode;
use sp_allocator::FreeingBumpHeapAllocator;
use tokio::sync::mpsc;
use wasmtime::{AsContextMut, Memory, StoreContextMut, Table};

fn checked_range(offset: usize, len: usize, max: usize) -> Option<Range<usize>> {
    let end = offset.checked_add(len)?;
    (end <= max).then(|| offset..end)
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

#[derive(Default)]
pub(crate) struct StoreData {
    pub(crate) memory: Option<Memory>,
    pub(crate) table: Option<Table>,
    pub(crate) allocator: Option<FreeingBumpHeapAllocator>,
    pub(crate) promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
}

impl StoreData {
    pub(crate) fn memory(&self) -> Memory {
        self.memory
            .expect("memory is initialized before host calls; qed")
    }
}

pub(crate) struct MemoryWrapper<'a> {
    caller: StoreContextMut<'a, StoreData>,
    memory: Memory,
}

// TODO: return results for mem accesses.
impl MemoryWrapper<'_> {
    pub fn decode_by_val<D: Decode>(&self, ptr_len: i64) -> D {
        let mut slice = self.slice_by_val(ptr_len);
        D::decode(&mut slice).unwrap()
    }

    pub fn decode<D: Decode>(&self, ptr: usize, len: usize) -> D {
        let mut slice = self.slice(ptr, len);
        D::decode(&mut slice).unwrap()
    }

    pub fn slice_by_val(&self, ptr_len: i64) -> &[u8] {
        let (ptr, len) = unpack_i64_to_u32(ptr_len);
        self.slice(ptr as usize, len as usize)
    }

    pub fn array<const N: usize>(&self, ptr: usize) -> [u8; N] {
        let slice = self.slice(ptr, N);
        slice.try_into().expect("infallible")
    }

    pub fn slice(&self, ptr: usize, len: usize) -> &[u8] {
        self.memory
            .data(&self.caller)
            .get(ptr..)
            .and_then(|s| s.get(..len))
            .unwrap()
    }

    pub fn slice_mut(&mut self, ptr: usize, len: usize) -> &mut [u8] {
        self.memory
            .data_mut(&mut self.caller)
            .get_mut(ptr..)
            .and_then(|s| s.get_mut(..len))
            .unwrap()
    }
}

impl sp_allocator::Memory for MemoryWrapper<'_> {
    fn with_access_mut<R>(&mut self, run: impl FnOnce(&mut [u8]) -> R) -> R {
        run(self.memory.data_mut(&mut self.caller))
    }

    fn with_access<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
        run(self.memory.data(&self.caller))
    }

    fn grow(&mut self, additional: u32) -> Result<(), ()> {
        self.memory
            .grow(&mut self.caller, additional as u64)
            .map_err(|err| {
                log::error!("Failed to grow memory by {additional} pages: {err}");
            })
            .map(drop)
    }

    fn pages(&self) -> u32 {
        self.memory.size(&self.caller) as u32
    }

    fn max_pages(&self) -> Option<u32> {
        self.memory
            .ty(&self.caller)
            .maximum()
            .map(|pages| pages as u32)
    }
}

pub(crate) fn memory<'a>(caller: impl Into<StoreContextMut<'a, StoreData>>) -> MemoryWrapper<'a> {
    let caller = caller.into();
    let memory = caller.data().memory();
    MemoryWrapper { caller, memory }
}

pub(crate) struct Allocator<'a> {
    memory: MemoryWrapper<'a>,
    allocator: Option<FreeingBumpHeapAllocator>,
}

impl Allocator<'_> {
    pub fn allocate(&mut self, size: u32) -> Result<u32, sp_allocator::Error> {
        self.allocator
            .as_mut()
            .unwrap()
            .allocate(&mut self.memory, size)
            .map(Into::into)
    }

    pub fn deallocate(&mut self, ptr: u32) -> Result<(), sp_allocator::Error> {
        self.allocator
            .as_mut()
            .unwrap()
            .deallocate(&mut self.memory, ptr.into())
    }
}

impl Drop for Allocator<'_> {
    fn drop(&mut self) {
        self.memory.caller.data_mut().allocator = self.allocator.take();
    }
}

pub(crate) fn allocator<'a>(caller: impl Into<StoreContextMut<'a, StoreData>>) -> Allocator<'a> {
    let mut caller = caller.into();
    let allocator = caller
        .data_mut()
        .allocator
        .take()
        .expect("allocator is available during wasm calls; qed");

    Allocator {
        memory: memory(caller),
        allocator: Some(allocator),
    }
}
