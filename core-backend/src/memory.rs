// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
    error::{
        BackendSyscallError, RunFallibleError, TrapExplanation, UndefinedTerminationReason,
        UnrecoverableMemoryError,
    },
    runtime::CallerWrap,
    state::HostState,
    BackendExternalities,
};
use alloc::vec::Vec;
use codec::{Decode, DecodeAll, MaxEncodedLen};
use core::{marker::PhantomData, mem, mem::MaybeUninit, slice};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Externalities,
    memory::{HostPointer, Memory, MemoryError, MemoryInterval},
    pages::{PageNumber, PageU32Size, WasmPage},
};
use gear_core_errors::MemoryError as FallibleMemoryError;
use gear_lazy_pages_common::ProcessAccessError;
use gear_sandbox::{default_executor::Store, AsContextExt, SandboxMemory};

pub type ExecutorMemory = gear_sandbox::default_executor::Memory;

pub(crate) struct MemoryWrapRef<'a, Caller, Memory> {
    pub caller: &'a mut Caller,
    pub memory: Memory,
}

impl<Caller, Ext, Mem> Memory for MemoryWrapRef<'_, Caller, Mem>
where
    Caller: AsContextExt<State = HostState<Ext, Mem>>,
    Ext: BackendExternalities,
    Mem: SandboxMemory<Caller::State>,
{
    type GrowError = gear_sandbox::Error;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory.grow(self.caller, pages.raw()).map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.size(self.caller))
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(self.caller, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(self.caller, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.get_buff(self.caller) as HostPointer
    }
}

/// Wrapper for executor memory.
pub struct MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    pub(crate) memory: ExecutorMemory,
    pub(crate) store: Store<HostState<Ext, ExecutorMemory>>,
}

impl<Ext> MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    /// Wrap [`ExecutorMemory`] for Memory trait.
    pub fn new(memory: ExecutorMemory, store: Store<HostState<Ext, ExecutorMemory>>) -> Self {
        MemoryWrap { memory, store }
    }

    pub(crate) fn into_store(self) -> Store<HostState<Ext, ExecutorMemory>> {
        self.store
    }
}

/// Memory interface for the allocator.
impl<Ext> Memory for MemoryWrap<Ext>
where
    Ext: Externalities + 'static,
{
    type GrowError = gear_sandbox::Error;

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        self.memory.grow(&mut self.store, pages.raw()).map(|_| ())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new(self.memory.size(&self.store))
            .expect("Unexpected backend behavior: wasm size is bigger then u32::MAX")
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .write(&mut self.store, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory
            .read(&self.store, offset, buffer)
            .map_err(|_| MemoryError::AccessOutOfBounds)
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
        self.memory.get_buff(&mut self.store) as HostPointer
    }
}

#[derive(Debug, Clone, derive_more::From)]
pub(crate) enum MemoryAccessError {
    Memory(MemoryError),
    ProcessAccess(ProcessAccessError),
    RuntimeBuffer(RuntimeBufferSizeError),
    // TODO: remove #2164
    Decode,
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
            MemoryAccessError::RuntimeBuffer(RuntimeBufferSizeError) => {
                TrapExplanation::UnrecoverableExt(
                    UnrecoverableMemoryError::RuntimeAllocOutOfBounds.into(),
                )
                .into()
            }
            // TODO: In facts thats legacy from lazy pages V1 implementation,
            // previously it was able to figure out that gas ended up in
            // pre-process charges: now we need actual counter type, so
            // it will be parsed and handled further (issue #3018).
            MemoryAccessError::ProcessAccess(ProcessAccessError::GasLimitExceeded) => {
                UndefinedTerminationReason::ProcessAccessErrorResourcesExceed
            }
            MemoryAccessError::Decode => unreachable!(),
        }
    }

    fn into_run_fallible_error(self) -> RunFallibleError {
        match self {
            MemoryAccessError::Memory(MemoryError::AccessOutOfBounds)
            | MemoryAccessError::ProcessAccess(ProcessAccessError::OutOfBounds) => {
                RunFallibleError::FallibleExt(FallibleMemoryError::AccessOutOfBounds.into())
            }
            MemoryAccessError::RuntimeBuffer(RuntimeBufferSizeError) => {
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
    pub(crate) reads: Vec<MemoryInterval>,
    pub(crate) writes: Vec<MemoryInterval>,
    _phantom: PhantomData<Caller>,
}

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
    Mem: SandboxMemory<Caller::State> + Clone + 'static,
{
    pub(crate) fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryRead { ptr, size }
    }

    pub(crate) fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        let size = mem::size_of::<T>() as u32;
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryReadAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    pub(crate) fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T> {
        let size = T::max_encoded_len() as u32;
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryReadDecoded {
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
        let size = mem::size_of::<T>() as u32;
        if size > 0 {
            self.writes.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryWriteAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    /// Call pre-processing of registered memory accesses.
    pub(crate) fn pre_process<'a>(
        self,
        ctx: &'a mut CallerWrap<'_, Caller>,
    ) -> Result<MemoryAccessIo<MemoryWrapRef<'a, Caller, Mem>>, MemoryAccessError> {
        let mut gas_counter = ctx.state_mut().ext.define_current_counter();

        let res = Ext::pre_process_memory_accesses(&self.reads, &self.writes, &mut gas_counter);

        ctx.state_mut().ext.decrease_current_counter_to(gas_counter);

        res?;

        let memory = ctx.state_mut().memory.clone();
        let memory = MemoryWrapRef {
            memory,
            caller: ctx.caller,
        };

        Ok(MemoryAccessIo { memory })
    }
}

/// Memory access writer and reader.
///
/// See [`MemoryAccessRegistry`].
pub(crate) struct MemoryAccessIo<Mem> {
    memory: Mem,
}

impl<Mem> MemoryAccessIo<Mem>
where
    Mem: Memory,
{
    pub(crate) fn read(&self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        let buff = if read.size == 0 {
            Vec::new()
        } else {
            let mut buff = RuntimeBuffer::try_new_default(read.size as usize)?.into_vec();
            self.memory.read(read.ptr, &mut buff)?;
            buff
        };
        Ok(buff)
    }

    pub(crate) fn read_as<T: Sized>(
        &self,
        read: WasmMemoryReadAs<T>,
    ) -> Result<T, MemoryAccessError> {
        let mut buf = MaybeUninit::<T>::uninit();

        let size = mem::size_of::<T>();
        if size > 0 {
            // # Safety:
            //
            // Usage of mutable slice is safe for the same reason from `write_as`.
            // `MaybeUninit` is presented on stack as a contiguous sequence of bytes.
            //
            // It's also safe to construct T from any bytes, because we use the fn
            // only for reading primitive const-size types that are `[repr(C)]`,
            // so they always represented from a sequence of bytes.
            //
            // Bytes in memory are always stored continuously and without paddings, properly
            // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
            let mut_slice = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, size) };

            self.memory.read(read.ptr, mut_slice)?;
        }
        Ok(unsafe { buf.assume_init() })
    }

    pub(crate) fn read_decoded<T: Decode + MaxEncodedLen>(
        &self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        let size = T::max_encoded_len();
        let buff = if size == 0 {
            Vec::new()
        } else {
            let mut buff = RuntimeBuffer::try_new_default(size)?.into_vec();
            self.memory.read(read.ptr, &mut buff)?;
            buff
        };
        let decoded = T::decode_all(&mut &buff[..]).map_err(|_| MemoryAccessError::Decode)?;
        Ok(decoded)
    }

    pub(crate) fn write(
        &mut self,
        write: WasmMemoryWrite,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError> {
        if buff.len() != write.size as usize {
            unreachable!("Backend bug error: buffer size is not equal to registered buffer size");
        }

        if write.size == 0 {
            Ok(())
        } else {
            self.memory.write(write.ptr, buff).map_err(Into::into)
        }
    }

    pub(crate) fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        let size = mem::size_of::<T>();
        if size > 0 {
            // # Safety:
            //
            // A given object is `Sized` and we own them in the context of calling this
            // function (it's on stack), it's safe to take ptr on the object and
            // represent it as slice.
            // Object will be dropped after `memory.write`
            // finished execution, and no one will rely on this slice.
            //
            // Bytes in memory are always stored continuously and without paddings, properly
            // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
            let slice = unsafe { slice::from_raw_parts(&obj as *const T as *const u8, size) };

            self.memory.write(write.ptr, slice).map_err(Into::into)
        } else {
            Ok(())
        }
    }
}

/// Read static size type access wrapper.
#[must_use]
pub(crate) struct WasmMemoryReadAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Read decoded type access wrapper.
#[must_use]
pub(crate) struct WasmMemoryReadDecoded<T: Decode + MaxEncodedLen> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Read access wrapper.
#[must_use]
pub(crate) struct WasmMemoryRead {
    ptr: u32,
    size: u32,
}

/// Write static size type access wrapper.
#[must_use]
pub(crate) struct WasmMemoryWriteAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Write access wrapper.
#[must_use]
pub(crate) struct WasmMemoryWrite {
    ptr: u32,
    size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mock::{MockExt, MockMemory, PreProcessMemoryAccesses},
        state::State,
    };
    use codec::Encode;
    use gear_core::pages::WASM_PAGE_SIZE;
    use gear_sandbox::SandboxStore;

    type MemoryAccessRegistry =
        crate::memory::MemoryAccessRegistry<Store<HostState<MockExt, MockMemory>>>;
    type MemoryAccessIo<'a> = crate::memory::MemoryAccessIo<
        MemoryWrapRef<'a, Store<HostState<MockExt, MockMemory>>, MockMemory>,
    >;

    #[derive(Encode, Decode, MaxEncodedLen)]
    #[codec(crate = codec)]
    struct ZeroSizeStruct;

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

        let accesses = PreProcessMemoryAccesses::take();
        assert_eq!(accesses.reads.len(), 1);
    }

    #[test]
    fn test_pre_process_with_only_writes() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let _write = registry.register_write(0, 10);

        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let accesses = PreProcessMemoryAccesses::take();
        assert_eq!(accesses.writes.len(), 1);
    }

    #[test]
    fn test_pre_process_with_reads_and_writes() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read(0, 10);
        let _write = registry.register_write(10, 20);

        let _io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let accesses = PreProcessMemoryAccesses::take();
        assert_eq!(accesses.reads.len(), 1);
        assert_eq!(accesses.writes.len(), 1);
    }

    #[test]
    fn test_read_of_zero_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read(0, 0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read(read).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.read_attempt_count(), 0);
    }

    #[test]
    fn test_read_of_zero_size_struct() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_as::<ZeroSizeStruct>(0);

        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_as(read).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.read_attempt_count(), 0);
    }

    #[test]
    fn test_read_of_zero_size_encoded_value() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_decoded::<ZeroSizeStruct>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_decoded(read).unwrap();
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
        io.read(read).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.read_attempt_count(), 1);
    }

    #[test]
    fn test_read_with_valid_memory_access() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        memory.write(0, &[5u8; 10]).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read(0, 10);

        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let vec = io.read(read).unwrap();
        assert_eq!(vec.as_slice(), &[5u8; 10]);
    }

    #[test]
    fn test_read_decoded_with_valid_encoded_data() {
        #[derive(Encode, Decode, Debug, PartialEq)]
        #[codec(crate = codec)]
        struct MockEncodeData {
            data: u64,
        }

        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let encoded = MockEncodeData { data: 1234 }.encode();
        memory.write(0, &encoded).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_decoded::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let data: u64 = io.read_decoded(read).unwrap();
        assert_eq!(data, 1234u64);
    }

    #[test]
    fn test_read_decoded_with_invalid_encoded_data() {
        #[derive(Debug)]
        struct InvalidDecode {}

        impl Decode for InvalidDecode {
            fn decode<T>(_input: &mut T) -> Result<Self, codec::Error> {
                Err("Invalid decoding".into())
            }
        }

        impl Encode for InvalidDecode {
            fn encode_to<T: codec::Output + ?Sized>(&self, _dest: &mut T) {}
        }

        impl MaxEncodedLen for InvalidDecode {
            fn max_encoded_len() -> usize {
                0
            }
        }

        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let encoded = alloc::vec![7u8; WASM_PAGE_SIZE];
        memory.write(0, &encoded).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_decoded::<InvalidDecode>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_decoded::<InvalidDecode>(read).unwrap_err();
    }

    #[test]
    fn test_read_decoded_reading_error() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);
        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read_decoded::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_decoded::<u64>(WasmMemoryReadDecoded {
            ptr: u32::MAX,
            _phantom: PhantomData,
        })
        .unwrap_err();
    }

    #[test]
    fn test_read_as_with_valid_data() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let memory = &mut caller_wrap.state_mut().memory;
        *memory = MockMemory::new(1);
        let encoded = 1234u64.to_le_bytes();
        memory.write(0, &encoded).unwrap();

        let mut registry = MemoryAccessRegistry::default();
        let read = registry.register_read_as::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        let decoded = io.read_as::<u64>(read).unwrap();
        assert_eq!(decoded, 1234);
    }

    #[test]
    fn test_read_as_with_invalid_pointer() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let _read = registry.register_read_as::<u64>(0);
        let io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.read_as::<u128>(WasmMemoryReadAs {
            ptr: u32::MAX,
            _phantom: PhantomData,
        })
        .unwrap_err();
    }

    #[test]
    fn test_write_of_zero_size_buf() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write(0, 0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write(write, &[]).unwrap();

        assert_eq!(caller_wrap.state_mut().memory.write_attempt_count(), 0);
    }

    #[test]
    fn test_write_of_zero_size_struct() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write_as::<ZeroSizeStruct>(0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write_as(write, ZeroSizeStruct).unwrap();

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
        io.write(write, &[]).unwrap();
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
        io.write(write, &buffer).unwrap();

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
        io.write(write, &buffer).unwrap();
    }

    #[test]
    fn test_write_as_with_zero_size_object() {
        let mut store = new_store();
        let mut caller_wrap = CallerWrap::new(&mut store);
        caller_wrap.state_mut().memory = MockMemory::new(1);

        let mut registry = MemoryAccessRegistry::default();
        let write = registry.register_write_as::<u32>(0);
        let mut io: MemoryAccessIo = registry.pre_process(&mut caller_wrap).unwrap();
        io.write_as(write, 0).unwrap();
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
            WasmMemoryWriteAs {
                ptr: 0,
                _phantom: PhantomData,
            },
            1u8,
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
            WasmMemoryWriteAs {
                ptr: WASM_PAGE_SIZE as u32,
                _phantom: PhantomData,
            },
            7u8,
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
    fn test_register_read_of_zero_size_struct() {
        let mut mem_access_manager = MemoryAccessRegistry::default();

        let _read = mem_access_manager.register_read_as::<ZeroSizeStruct>(142);

        assert_eq!(mem_access_manager.reads.len(), 0);
    }

    #[test]
    fn test_register_read_of_zero_size_encoded_value() {
        let mut mem_access_manager = MemoryAccessRegistry::default();

        let _read = mem_access_manager.register_read_decoded::<ZeroSizeStruct>(142);

        assert_eq!(mem_access_manager.reads.len(), 0);
    }

    #[test]
    fn test_register_read_as_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, core::mem::size_of::<u8>() as u32);
    }

    #[test]
    fn test_register_read_as_with_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, core::mem::size_of::<u8>() as u32);
    }

    #[derive(Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
    #[codec(crate = codec)]
    struct TestStruct {
        a: u32,
        b: u64,
    }

    #[test]
    fn test_register_read_decoded_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_decoded::<TestStruct>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, TestStruct::max_encoded_len() as u32);
    }

    #[test]
    fn test_register_read_decoded_with_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_read_decoded::<TestStruct>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 1);
        assert_eq!(registry.writes.len(), 0);
        assert_eq!(registry.reads[0].offset, 0);
        assert_eq!(registry.reads[0].size, TestStruct::max_encoded_len() as u32);
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
    fn test_register_write_of_zero_size_struct() {
        let mut mem_access_manager = MemoryAccessRegistry::default();

        let _write = mem_access_manager.register_write_as::<ZeroSizeStruct>(142);

        assert_eq!(mem_access_manager.writes.len(), 0);
    }

    #[test]
    fn test_register_write_as_with_valid_interval() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 1);
        assert_eq!(registry.writes[0].offset, 0);
        assert_eq!(registry.writes[0].size, core::mem::size_of::<u8>() as u32);
    }

    #[test]
    fn test_register_write_as_with_zero_size() {
        let mut registry = MemoryAccessRegistry::default();

        let result = registry.register_write_as::<u8>(0);

        assert_eq!(result.ptr, 0);
        assert_eq!(registry.reads.len(), 0);
        assert_eq!(registry.writes.len(), 1);
        assert_eq!(registry.writes[0].offset, 0);
        assert_eq!(registry.writes[0].size, core::mem::size_of::<u8>() as u32);
    }
}
