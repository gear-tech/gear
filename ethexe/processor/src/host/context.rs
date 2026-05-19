// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::BoundPromiseSink;
use ethexe_db::CASDatabase;
use ethexe_runtime_common::{pack_u32_to_i64, unpack_i64_to_u32};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use sp_allocator::FreeingBumpHeapAllocator;
use wasmtime::{AsContext, AsContextMut, Memory, StoreContextMut, Table};

pub(crate) struct StoreData {
    pub(crate) memory: Option<Memory>,
    pub(crate) table: Option<Table>,
    pub(crate) allocator: Option<FreeingBumpHeapAllocator>,
    pub(crate) db: Box<dyn CASDatabase>,
    pub(crate) promise_sink: Option<BoundPromiseSink>,
}

impl StoreData {
    pub(crate) fn memory(&self) -> Memory {
        self.memory
            .expect("memory is initialized before host calls; qed")
    }
}

pub(crate) struct MemoryWrapper<C> {
    caller: C,
    memory: Memory,
}

// TODO: return results for mem accesses.
impl<C> MemoryWrapper<C>
where
    C: AsContext<Data = StoreData>,
{
    pub fn decode_by_val<D: Decode>(&self, ptr_len: i64) -> D {
        let mut slice = self.slice_by_val(ptr_len);
        D::decode(&mut slice).unwrap()
    }

    pub fn decode_by_max_len<D: Decode + MaxEncodedLen>(&self, ptr: u32) -> D {
        debug_assert!(D::max_encoded_len() < u32::MAX as usize);

        let mut slice = self.slice(ptr, D::max_encoded_len() as u32).unwrap();
        D::decode(&mut slice).unwrap()
    }

    pub fn slice_by_val(&self, ptr_len: i64) -> &[u8] {
        let (ptr, len) = unpack_i64_to_u32(ptr_len);
        self.slice(ptr, len).unwrap()
    }

    pub fn slice(&self, ptr: u32, len: u32) -> Option<&[u8]> {
        self.memory
            .data(&self.caller)
            .get(ptr as usize..)
            .and_then(|s| s.get(..len as usize))
    }
}

impl<C> MemoryWrapper<C>
where
    C: AsContextMut<Data = StoreData>,
{
    pub fn allocate_and_write_val(&mut self, data: impl Encode) -> i64 {
        self.allocate_and_write_val_raw(data.encode())
    }

    pub fn allocate_and_write_val_raw(&mut self, data: impl AsRef<[u8]>) -> i64 {
        let data = data.as_ref();
        let len = data.len();

        let ptr: u32 = allocator(&mut self.caller).allocate(len as u32).unwrap();
        self.memory
            .write(&mut self.caller, ptr as usize, data)
            .unwrap();

        pack_u32_to_i64(ptr, len as u32)
    }

    pub fn slice_mut(&mut self, ptr: u32, len: u32) -> Option<&mut [u8]> {
        self.memory
            .data_mut(&mut self.caller)
            .get_mut(ptr as usize..)
            .and_then(|s| s.get_mut(..len as usize))
    }
}

impl<C: AsContextMut> sp_allocator::Memory for MemoryWrapper<C> {
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

pub(crate) fn memory<C>(caller: C) -> MemoryWrapper<C>
where
    C: AsContext<Data = StoreData>,
{
    let memory = caller.as_context().data().memory();
    MemoryWrapper { caller, memory }
}

pub(crate) struct Allocator<'a> {
    memory: MemoryWrapper<StoreContextMut<'a, StoreData>>,
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
