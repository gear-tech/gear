// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! sp-sandbox extensions for memory.

use crate::{
    BackendExternalities,
    error::{
        BackendSyscallError, RunFallibleError, TrapExplanation, UndefinedTerminationReason,
        UnrecoverableMemoryError,
    },
    runtime::CallerWrap,
    state::HostState,
};
use alloc::{format, vec::Vec};
use bytemuck::Pod;
use core::marker::PhantomData;
use gear_core::{
    buffer::RuntimeBuffer,
    limited::LimitedVecError,
    memory::{HostPointer, Memory, MemoryError, MemoryInterval},
    pages::WasmPagesAmount,
};
use gear_core_errors::MemoryError as FallibleMemoryError;
use gear_lazy_pages_common::ProcessAccessError;
use gear_sandbox::{AsContextExt, SandboxMemory};

pub type ExecutorMemory = gear_sandbox::default_executor::Memory;

#[derive(Debug, Clone, derive_more::From)]
pub struct BackendMemory<Mem> {
    inner: Mem,
}

impl<Mem> BackendMemory<Mem> {
    pub fn into_inner(self) -> Mem {
        self.inner
    }
}

impl<Caller, Ext, Mem> Memory<Caller> for BackendMemory<Mem>
where
    Caller: AsContextExt<State = HostState<Ext, BackendMemory<Mem>>>,
    Mem: SandboxMemory<HostState<Ext, BackendMemory<Mem>>>,
{
    type GrowError = gear_sandbox::Error;

    fn grow(&self, ctx: &mut Caller, pages: WasmPagesAmount) -> Result<(), Self::GrowError> {
        self.inner.grow(ctx, pages.into()).map(|_| ())
    }

    fn size(&self, ctx: &Caller) -> WasmPagesAmount {
        WasmPagesAmount::try_from(self.inner.size(ctx)).unwrap_or_else(|err| {
            let err_msg = format!(
                "BackendMemory::size: wasm size is bigger than possible in 32-bits address space. \
                Got error - {err:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        })
    }

    fn write(&self, ctx: &mut Caller, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.inner
            .write(ctx, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, ctx: &Caller, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.inner
            .read(ctx, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&self, ctx: &Caller) -> HostPointer {
        unsafe { self.inner.get_buff(ctx) as HostPointer }
    }
}

#[derive(Debug, Copy, Clone, derive_more::From)]
pub(crate) enum MemoryAccessError {
    Memory(MemoryError),
    ProcessAccess(ProcessAccessError),
    RuntimeBuffer(LimitedVecError),
}

impl BackendSyscallError for MemoryAccessError {
    fn into_termination_reason(self) -> UndefinedTerminationReason {
        match self {
            MemoryAccessError::ProcessAccess(ProcessAccessError::OutOfBounds)
            | MemoryAccessError::Memory(MemoryError::AccessOutOfBounds) => {
                TrapExplanation::UnrecoverableExt(
                    UnrecoverableMemoryError::AccessOutOfBounds.into(),
                )
                .into()
            }
            MemoryAccessError::RuntimeBuffer(LimitedVecError) => TrapExplanation::UnrecoverableExt(
                UnrecoverableMemoryError::RuntimeAllocOutOfBounds.into(),
            )
            .into(),
            // TODO: In facts thats legacy from lazy pages V1 implementation,
            // previously it was able to figure out that gas ended up in
            // pre-process charges: now we need actual counter type, so
            // it will be parsed and handled further (issue #3018).
            MemoryAccessError::ProcessAccess(ProcessAccessError::GasLimitExceeded) => {
                UndefinedTerminationReason::ProcessAccessErrorResourcesExceed
            }
        }
    }

    fn into_run_fallible_error(self) -> RunFallibleError {
        match self {
            MemoryAccessError::Memory(MemoryError::AccessOutOfBounds)
            | MemoryAccessError::ProcessAccess(ProcessAccessError::OutOfBounds) => {
                RunFallibleError::FallibleExt(FallibleMemoryError::AccessOutOfBounds.into())
            }
            MemoryAccessError::RuntimeBuffer(LimitedVecError) => {
                RunFallibleError::FallibleExt(FallibleMemoryError::RuntimeAllocOutOfBounds.into())
            }
            e => RunFallibleError::UndefinedTerminationReason(e.into_termination_reason()),
        }
    }
}

/// Memory access registry.
///
/// Allows to pre-register memory accesses, and pre-process them together in
/// [`BackendExternalities::pre_process_memory_accesses`].
/// And only then do actual read/write in type-safe way.
///
/// ```rust,ignore
/// # let ctx: () = ();
/// let registry = MemoryAccessRegistry::default();
/// let read1 = registry.new_read(10, 20);
/// let read2 = registry.new_read_as::<u128>(100);
/// let write1 = registry.new_write_as::<usize>(190);
///
/// // Pre-process all registered memory accesses
/// let io = registry.pre_process(ctx);
///
/// let value_u128 = io.read_as(read2).unwrap();
///
/// let value1 = io.read(read1).unwrap();
/// io.write_as(write1, 111).unwrap();
/// ```
#[derive(Debug)]
pub(crate) struct MemoryAccessRegistry<Caller> {
    reads: Vec<MemoryInterval>,
    writes: Vec<MemoryInterval>,
    _phantom: PhantomData<Caller>,
}

// TODO: remove this public constructor and use extractors in `funcs.rs` instead (#3891)
impl<Caller> Default for MemoryAccessRegistry<Caller> {
    fn default() -> Self {
        Self {
            reads: Default::default(),
            writes: Default::default(),
            _phantom: PhantomData,
        }
    }
}

impl<Caller, Ext, Mem> MemoryAccessRegistry<Caller>
where
    Caller: AsContextExt<State = HostState<Ext, Mem>>,
    Ext: BackendExternalities + 'static,
    Mem: Memory<Caller> + Clone + 'static,
{
    pub(crate) fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryRead { ptr, size }
    }

    pub(crate) fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        debug_assert!(
            core::mem::size_of::<T>() != 0,
            "Reading a ZST from memory is likely a bug"
        );

        let size = size_of::<T>() as u32;

        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryReadAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        if size > 0 {
            self.writes.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryWrite { ptr, size }
    }

    pub(crate) fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        let size = size_of::<T>() as u32;

        debug_assert!(
            core::mem::size_of::<T>() != 0,
            "Writing a ZST into memory is likely a bug"
        );

        if size > 0 {
            self.writes.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryWriteAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    /// Call pre-processing of registered memory accesses.
    pub(crate) fn pre_process(
        self,
        ctx: &mut CallerWrap<Caller>,
    ) -> Result<MemoryAccessIo<Caller, Mem>, MemoryAccessError> {
        let ext = ctx.ext_mut();
        let mut gas_counter = ext.define_current_counter();

        let res = ext.pre_process_memory_accesses(&self.reads, &self.writes, &mut gas_counter);

        ext.decrease_current_counter_to(gas_counter);

        res?;

        let memory = ctx.state_mut().memory.clone();
        Ok(MemoryAccessIo {
            memory,
            _phantom: PhantomData,
        })
    }
}

/// Memory access writer and reader.
///
/// See [`MemoryAccessRegistry`].
pub(crate) struct MemoryAccessIo<Context, Mem> {
    memory: Mem,
    _phantom: PhantomData<Context>,
}

impl<Context, Mem> MemoryAccessIo<Context, Mem>
where
    Mem: Memory<Context>,
{
    pub(crate) fn read(
        &self,
        ctx: &mut CallerWrap<Context>,
        read: WasmMemoryRead,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        let buff = if read.size == 0 {
            Vec::new()
        } else {
            let mut buff = RuntimeBuffer::try_repeat(0, read.size as usize)?.into_vec();
            self.memory.read(ctx.caller, read.ptr, &mut buff)?;
            buff
        };
        Ok(buff)
    }

    pub(crate) fn read_as<T: Pod>(
        &self,
        ctx: &mut CallerWrap<Context>,
        read: WasmMemoryReadAs<T>,
    ) -> Result<T, MemoryAccessError> {
        let mut value = bytemuck::zeroed();

        debug_assert_ne!(
            core::mem::size_of::<T>(),
            0,
            "Reading a ZST from memory is likely a bug"
        );

        if core::mem::size_of::<T>() != 0 {
            self.memory
                .read(ctx.caller, read.ptr, bytemuck::bytes_of_mut(&mut value))?;
        }

        Ok(value)
    }

    pub(crate) fn write(
        &mut self,
        ctx: &mut CallerWrap<Context>,
        write: WasmMemoryWrite,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError> {
        if buff.len() != write.size as usize {
            let err_msg = format!(
                "MemoryAccessIo::write: Backend bug error, buffer size is not equal to registered buffer size. \
                write.ptr - {}, write.size - {}, buff.len - {}",
                write.ptr,
                write.size,
                buff.len()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        if write.size == 0 {
            Ok(())
        } else {
            self.memory
                .write(ctx.caller, write.ptr, buff)
                .map_err(Into::into)
        }
    }

    pub(crate) fn write_as<T: Pod>(
        &mut self,
        ctx: &mut CallerWrap<Context>,
        write: WasmMemoryWriteAs<T>,
        obj: &T,
    ) -> Result<(), MemoryAccessError> {
        debug_assert_ne!(
            core::mem::size_of::<T>(),
            0,
            "Writing a ZST into memory is likely a bug"
        );

        self.memory
            .write(ctx.caller, write.ptr, bytemuck::bytes_of(obj))?;

        Ok(())
    }
}

/// Read access wrapper.
#[must_use]
pub(crate) struct WasmMemoryRead {
    pub ptr: u32,
    pub size: u32,
}

/// Read static size type access wrapper.
#[must_use]
pub(crate) struct WasmMemoryReadAs<T> {
    pub ptr: u32,
    _phantom: PhantomData<T>,
}

/// Write static size type access wrapper.
#[must_use]
pub(crate) struct WasmMemoryWriteAs<T> {
    pub ptr: u32,
    _phantom: PhantomData<T>,
}

/// Write access wrapper.
#[must_use]
pub(crate) struct WasmMemoryWrite {
    pub ptr: u32,
    pub size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::{MockExt, MockMemory},
        state::State,
    };
    use bytemuck::Zeroable;
    use gear_core::pages::WasmPage;
    use gear_sandbox::{SandboxStore, default_executor::Store};
    use parity_scale_codec::Encode;

    type MemoryAccessRegistry =
        crate::memory::MemoryAccessRegistry<Store<HostState<MockExt, MockMemory>>>;
    type MemoryAccessIo<'a> =
        crate::memory::MemoryAccessIo<Store<HostState<MockExt, MockMemory>>, MockMemory>;

    fn new_store() -> Store<HostState<MockExt, MockMemory>> {
        Store::new(Some(State {
            ext: MockExt::default(),
            memory: MockMemory::new(0),
            termination_reason: UndefinedTerminationReason::ProcessAccessErrorResourcesExceed,
        }))
    }

    #[test]
    fn test_pre_process_with_no_accesses() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let registry = MemoryAccessRegistry::default();
        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
    }

    #[test]
    fn test_pre_process_with_only_reads() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read(0, 10);

        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();

        let (reads, writes) = caller_wrap.ext_mut().take_pre_process_accesses();
        assert_eq!(reads.len(), 1);
        assert_eq!(writes, []);
    }

    #[test]
    fn test_pre_process_with_only_writes() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let _write = registry.register_write(0, 10);

        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let (reads, writes) = caller_wrap.ext_mut().take_pre_process_accesses();
        assert_eq!(reads, []);
        assert_eq!(writes.len(), 1);
    }

    #[test]
    fn test_pre_process_with_reads_and_writes() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read(0, 10);
        let _write = registry.register_write(10, 20);

        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let (reads, writes) = caller_wrap.ext_mut().take_pre_process_accesses();
        assert_eq!(reads.len(), 1);
        assert_eq!(writes.len(), 1);
    }

    #[test]
    fn test_read_of_zero_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read(0, 0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read(&mut caller_wrap, read).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.read_attempt_count(), 0);
    }

    #[test]
    fn test_read_of_some_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read(0, 10);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read(&mut caller_wrap, read).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.read_attempt_count(), 1);
    }

    #[test]
    fn test_read_with_valid_memory_access() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let buffer = &[5u8; 10];
        memory.write(&mut (), 0, buffer).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read(0, 10);

        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let vec = io.read(&mut caller_wrap, read).unwrap();
        assert_eq!(vec.as_slice(), &[5u8; 10]);
    }

    #[test]
    fn test_read_as_reading_error() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);
        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read_as::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_as::<u64>(
            &mut caller_wrap,
            WasmMemoryReadAs {
                ptr: u32::MAX,
                _phantom: PhantomData,
            },
        )
        .unwrap_err();
    }

    #[test]
    fn test_read_as_with_valid_data() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let encoded = 1234u64.to_le_bytes();
        memory.write(&mut (), 0, &encoded).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_as::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let decoded = io.read_as::<u64>(&mut caller_wrap, read).unwrap();
        assert_eq!(decoded, 1234);
    }

    #[test]
    fn test_read_as_struct() {
        #[derive(Encode, Debug, PartialEq, Clone, Copy, Zeroable, Pod)]
        #[repr(C)]
        struct MockEncodeData {
            a: u64,
            b: u64,
            c: u32,
            d: u32,
        }

        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let original_data = MockEncodeData {
            a: 12,
            b: 34,
            c: 56,
            d: 78,
        };
        memory
            .write(&mut (), 0, bytemuck::bytes_of(&original_data))
            .unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_as::<MockEncodeData>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let data = io.read_as(&mut caller_wrap, read).unwrap();
        assert_eq!(data, original_data);
    }

    #[test]
    fn test_read_as_with_invalid_pointer() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read_as::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_as::<u128>(
            &mut caller_wrap,
            WasmMemoryReadAs {
                ptr: u32::MAX,
                _phantom: PhantomData,
            },
        )
        .unwrap_err();
    }

    #[test]
    fn test_write_of_zero_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write(0, 0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write(&mut caller_wrap, write, &[]).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.write_attempt_count(), 0);
    }

    #[test]
    #[should_panic(expected = "buffer size is not equal to registered buffer size")]
    fn test_write_with_zero_buffer_size() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write(0, 10);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write(&mut caller_wrap, write, &[]).unwrap();
    }

    #[test]
    fn test_write_of_some_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write(0, 10);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let buffer = [0u8; 10];
        io.write(&mut caller_wrap, write, &buffer).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.write_attempt_count(), 1);
    }

    #[test]
    #[should_panic = "buffer size is not equal to registered buffer size"]
    fn test_write_with_larger_buffer_size() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write(0, 10);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let buffer = [0u8; 20];
        io.write(&mut caller_wrap, write, &buffer).unwrap();
    }

    #[test]
    fn test_write_as_with_zero_size_object() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write_as::<u32>(0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write_as(&mut caller_wrap, write, &0).unwrap();
    }

    #[test]
    fn test_write_as_with_same_object_size() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let _write = registry.register_write_as::<u8>(0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write_as(
            &mut caller_wrap,
            WasmMemoryWriteAs {
                ptr: 0,
                _phantom: PhantomData,
            },
            &1u8,
        )
        .unwrap();
    }

    #[test]
    fn test_write_as_with_larger_object_size() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let _write = registry.register_write_as::<u8>(0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write_as(
            &mut caller_wrap,
            WasmMemoryWriteAs {
                ptr: WasmPage::SIZE,
                _phantom: PhantomData,
            },
            &7u8,
        )
        .unwrap_err();
    }

    #[test]
    fn test_register_read_of_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read(0, 10);

        assert_eq!(result.ptr, 0);
        assert_eq!(result.size, 10);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
    }

    #[test]
    fn test_register_read_of_zero_size_buf() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read(0, 0);

        assert_eq!(result.ptr, 0);
        assert_eq!(result.size, 0);
        assert_eq!(registry.reads.len(), 0);
    }

    #[test]
    fn test_register_read_as_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, size_of::<u8>() as u32);
    }

    #[test]
    fn test_register_read_as_with_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, size_of::<u8>() as u32);
    }

    #[repr(C, packed)]
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Zeroable, Pod)]
    struct TestStruct {
        a: u32,
        b: u64,
    }

    #[test]
    fn test_register_read_decoded_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<TestStruct>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(
            registry.reads[0].size,
            core::mem::size_of::<TestStruct>() as u32
        );
    }

    #[test]
    fn test_register_read_with_non_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<TestStruct>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(
            registry.reads[0].size,
            core::mem::size_of::<TestStruct>() as u32
        );
    }

    #[test]
    fn test_register_write_of_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write(0, 10);

        assert_eq!(result.ptr, 0);
        assert_eq!(result.size, 10);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 1);
    }

    #[test]
    fn test_register_write_of_zero_size_buf() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write(0, 0);

        assert_eq!(result.ptr, 0);
        assert_eq!(result.size, 0);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 0);
    }

    #[test]
    fn test_register_write_as_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 1);
        assert_eq!(registry.writes[0].offset, 0);
        assert_eq!(registry.writes[0].size, size_of::<u8>() as u32);
    }

    #[test]
    fn test_register_write_as_with_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 1);
        assert_eq!(registry.writes[0].offset, 0);
        assert_eq!(registry.writes[0].size, size_of::<u8>() as u32);
    }
}
