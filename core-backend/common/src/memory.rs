// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Work with WASM program memory in backends.

use crate::BackendExternalities;
use alloc::vec::Vec;
use core::{
    fmt::Debug,
    marker::PhantomData,
    mem,
    mem::{size_of, MaybeUninit},
    result::Result,
    slice,
};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    gas::GasLeft,
    memory::{Memory, MemoryInterval},
};
use gear_core_errors::MemoryError;
use scale_info::scale::{self, Decode, DecodeAll, Encode, MaxEncodedLen};

/// Memory access error during sys-call that lazy-pages have caught.
#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = scale)]
pub enum ProcessAccessError {
    OutOfBounds,
    GasLimitExceeded,
    GasAllowanceExceeded,
}

#[derive(Debug, Clone, derive_more::From)]
pub enum MemoryAccessError {
    #[from]
    Memory(MemoryError),
    #[from]
    RuntimeBuffer(RuntimeBufferSizeError),
    // TODO: remove #2164
    Decode,
    GasLimitExceeded,
    GasAllowanceExceeded,
}

impl From<ProcessAccessError> for MemoryAccessError {
    fn from(err: ProcessAccessError) -> Self {
        match err {
            ProcessAccessError::OutOfBounds => MemoryError::AccessOutOfBounds.into(),
            ProcessAccessError::GasLimitExceeded => Self::GasLimitExceeded,
            ProcessAccessError::GasAllowanceExceeded => Self::GasAllowanceExceeded,
        }
    }
}

/// Memory accesses recorder/registrar, which allow to register new accesses.
pub trait MemoryAccessRecorder {
    /// Register new read access.
    fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead;

    /// Register new read static size type access.
    fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T>;

    /// Register new read decoded type access.
    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T>;

    /// Register new write access.
    fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite;

    /// Register new write static size access.
    fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T>;
}

pub trait MemoryOwner {
    /// Read from owned memory to new byte vector.
    fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError>;

    /// Read from owned memory to new object `T`.
    fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError>;

    /// Read from owned memory and decoded data into object `T`.
    fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError>;

    /// Write data from `buff` to owned memory.
    fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError>;

    /// Write data from `obj` to owned memory.
    fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError>;
}

/// Memory access manager. Allows to pre-register memory accesses,
/// and pre-process, them together. For example:
/// ```ignore
/// let manager = MemoryAccessManager::default();
/// let read1 = manager.new_read(10, 20);
/// let read2 = manager.new_read_as::<u128>(100);
/// let write1 = manager.new_write_as::<usize>(190);
///
/// // First call of read or write interface leads to pre-processing of
/// // all already registered memory accesses, and clear `self.reads` and `self.writes`.
/// let value_u128 = manager.read_as(read2).unwrap();
///
/// // Next calls do not lead to access pre-processing.
/// let value1 = manager.read().unwrap();
/// manager.write_as(write1, 111).unwrap();
/// ```
#[derive(Debug)]
pub struct MemoryAccessManager<E> {
    // Contains non-zero length intervals only.
    pub(crate) reads: Vec<MemoryInterval>,
    pub(crate) writes: Vec<MemoryInterval>,
    pub(crate) _phantom: PhantomData<E>,
}

impl<E> Default for MemoryAccessManager<E> {
    fn default() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<E> MemoryAccessRecorder for MemoryAccessManager<E> {
    fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryRead { ptr, size }
    }

    fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        let size = size_of::<T>() as u32;
        if size > 0 {
            self.reads.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryReadAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    fn register_read_decoded<T: Decode + MaxEncodedLen>(
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

    fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        if size > 0 {
            self.writes.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryWrite { ptr, size }
    }

    fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        let size = size_of::<T>() as u32;
        if size > 0 {
            self.writes.push(MemoryInterval { offset: ptr, size });
        }
        WasmMemoryWriteAs {
            ptr,
            _phantom: PhantomData,
        }
    }
}

impl<E: BackendExternalities> MemoryAccessManager<E> {
    /// Call pre-processing of registered memory accesses. Clear `self.reads` and `self.writes`.
    pub(crate) fn pre_process_memory_accesses(
        &mut self,
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        if self.reads.is_empty() && self.writes.is_empty() {
            return Ok(());
        }

        let res = E::pre_process_memory_accesses(&self.reads, &self.writes, gas_left);

        self.reads.clear();
        self.writes.clear();

        res.map_err(Into::into)
    }

    /// Pre-process registered accesses if need and read data from `memory` to `buff`.
    fn read_into_buf<M: Memory>(
        &mut self,
        memory: &M,
        ptr: u32,
        buff: &mut [u8],
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_memory_accesses(gas_left)?;
        memory.read(ptr, buff).map_err(Into::into)
    }

    /// Pre-process registered accesses if need and read data from `memory` into new vector.
    pub fn read<M: Memory>(
        &mut self,
        memory: &M,
        read: WasmMemoryRead,
        gas_left: &mut GasLeft,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        let buff = if read.size == 0 {
            Vec::new()
        } else {
            let mut buff = RuntimeBuffer::try_new_default(read.size as usize)?.into_vec();
            self.read_into_buf(memory, read.ptr, &mut buff, gas_left)?;
            buff
        };
        Ok(buff)
    }

    /// Pre-process registered accesses if need and read and decode data as `T` from `memory`.
    pub fn read_decoded<M: Memory, T: Decode + MaxEncodedLen>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadDecoded<T>,
        gas_left: &mut GasLeft,
    ) -> Result<T, MemoryAccessError> {
        let size = T::max_encoded_len();
        let buff = if size == 0 {
            Vec::new()
        } else {
            let mut buff = RuntimeBuffer::try_new_default(size)?.into_vec();
            self.read_into_buf(memory, read.ptr, &mut buff, gas_left)?;
            buff
        };
        let decoded = T::decode_all(&mut &buff[..]).map_err(|_| MemoryAccessError::Decode)?;
        Ok(decoded)
    }

    /// Pre-process registered accesses if need and read data as `T` from `memory`.
    pub fn read_as<M: Memory, T: Sized>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadAs<T>,
        gas_left: &mut GasLeft,
    ) -> Result<T, MemoryAccessError> {
        self.pre_process_memory_accesses(gas_left)?;
        read_memory_as(memory, read.ptr).map_err(Into::into)
    }

    /// Pre-process registered accesses if need and write data from `buff` to `memory`.
    pub fn write<M: Memory>(
        &mut self,
        memory: &mut M,
        write: WasmMemoryWrite,
        buff: &[u8],
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        if buff.len() != write.size as usize {
            unreachable!("Backend bug error: buffer size is not equal to registered buffer size");
        }
        if write.size == 0 {
            Ok(())
        } else {
            self.pre_process_memory_accesses(gas_left)?;
            memory.write(write.ptr, buff).map_err(Into::into)
        }
    }

    /// Pre-process registered accesses if need and write `obj` data to `memory`.
    pub fn write_as<M: Memory, T: Sized>(
        &mut self,
        memory: &mut M,
        write: WasmMemoryWriteAs<T>,
        obj: T,
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_memory_accesses(gas_left)?;
        write_memory_as(memory, write.ptr, obj).map_err(Into::into)
    }
}

/// Writes object in given memory as bytes.
fn write_memory_as<T: Sized>(
    memory: &mut impl Memory,
    ptr: u32,
    obj: T,
) -> Result<(), MemoryError> {
    let size = mem::size_of::<T>();
    if size > 0 {
        // # Safety:
        //
        // Given object is `Sized` and we own them in the context of calling this
        // function (it's on stack), it's safe to take ptr on the object and
        // represent it as slice. Object will be dropped after `memory.write`
        // finished execution and no one will rely on this slice.
        //
        // Bytes in memory always stored continuously and without paddings, properly
        // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
        let slice = unsafe { slice::from_raw_parts(&obj as *const T as *const u8, size) };

        memory.write(ptr, slice)
    } else {
        Ok(())
    }
}

/// Reads bytes from given pointer to construct type T from them.
fn read_memory_as<T: Sized>(memory: &impl Memory, ptr: u32) -> Result<T, MemoryError> {
    let mut buf = MaybeUninit::<T>::uninit();

    let size = mem::size_of::<T>();
    if size > 0 {
        // # Safety:
        //
        // Usage of mutable slice is safe for the same reason from `write_memory_as`.
        // `MaybeUninit` is presented on stack with continuos sequence of bytes.
        //
        // It's also safe to construct T from any bytes, because we use the fn
        // only for reading primitive const-size types that are `[repr(C)]`,
        // so they always represented from sequence of bytes.
        //
        // Bytes in memory always stored continuously and without paddings, properly
        // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
        let mut_slice = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, size) };

        memory.read(ptr, mut_slice)?;
    }

    // # Safety:
    //
    // Assuming init is always safe here due to the fact that we read proper
    // amount of bytes from the wasm memory, which is never uninited: they may
    // be filled by zeroes or some trash (valid for our primitives used as T),
    // but always exist.
    Ok(unsafe { buf.assume_init() })
}

/// Read static size type access wrapper.
pub struct WasmMemoryReadAs<T> {
    pub(crate) ptr: u32,
    pub(crate) _phantom: PhantomData<T>,
}

/// Read decoded type access wrapper.
pub struct WasmMemoryReadDecoded<T: Decode + MaxEncodedLen> {
    pub(crate) ptr: u32,
    pub(crate) _phantom: PhantomData<T>,
}

/// Read access wrapper.
pub struct WasmMemoryRead {
    pub(crate) ptr: u32,
    pub(crate) size: u32,
}

/// Write static size type access wrapper.
pub struct WasmMemoryWriteAs<T> {
    pub(crate) ptr: u32,
    pub(crate) _phantom: PhantomData<T>,
}

/// Write access wrapper.
pub struct WasmMemoryWrite {
    pub(crate) ptr: u32,
    pub(crate) size: u32,
}
