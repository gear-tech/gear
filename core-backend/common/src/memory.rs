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

//! Work with WASM program memory in backends.

// TODO: make unit tests for `MemoryAccessManager` (issue #2068)

use core::{marker::PhantomData, mem::size_of};

use alloc::vec::Vec;
use codec::{Decode, DecodeAll, MaxEncodedLen};
use core::fmt::Debug;
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    memory::{Memory, MemoryInterval},
};
use gear_core_errors::MemoryError;

use crate::IntoExtInfo;

/// Memory access error.
#[derive(Debug, Clone, Copy)]
pub struct OutOfMemoryAccessError;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum MemoryAccessError {
    #[from]
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeBuffer(RuntimeBufferSizeError),
    #[display(fmt = "Cannot decode read memory")]
    DecodeError,
    #[display(fmt = "Buffer size {_0} is not equal to pre-registered size {_1}")]
    WrongBufferSize(usize, u32),
}

impl From<OutOfMemoryAccessError> for MemoryAccessError {
    fn from(_: OutOfMemoryAccessError) -> Self {
        MemoryAccessError::Memory(MemoryError::OutOfBounds)
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
pub struct MemoryAccessManager<E: Ext + IntoExtInfo<E::Error>> {
    reads: Vec<MemoryInterval>,
    writes: Vec<MemoryInterval>,
    _phantom: PhantomData<E>,
}

impl<E: Ext + IntoExtInfo<E::Error>> Default for MemoryAccessManager<E> {
    fn default() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<E: Ext + IntoExtInfo<E::Error>> MemoryAccessRecorder for MemoryAccessManager<E> {
    fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        self.reads.push(MemoryInterval { offset: ptr, size });
        WasmMemoryRead { ptr, size }
    }

    fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        self.reads.push(MemoryInterval {
            offset: ptr,
            size: size_of::<T>() as u32,
        });
        WasmMemoryReadAs {
            ptr,
            _phantom: PhantomData,
        }
    }

    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T> {
        self.reads.push(MemoryInterval {
            offset: ptr,
            size: T::max_encoded_len() as u32,
        });
        WasmMemoryReadDecoded {
            ptr,
            _phantom: PhantomData,
        }
    }

    fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        self.writes.push(MemoryInterval { offset: ptr, size });
        WasmMemoryWrite { ptr, size }
    }

    fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        self.writes.push(MemoryInterval {
            offset: ptr,
            size: size_of::<T>() as u32,
        });
        WasmMemoryWriteAs {
            ptr,
            _phantom: PhantomData,
        }
    }
}

impl<E: Ext + IntoExtInfo<E::Error>> MemoryAccessManager<E> {
    /// Call pre-processing of registered memory accesses. Clear `self.reads` and `self.writes`.
    fn pre_process_memory_accesses(&mut self) -> Result<(), MemoryAccessError> {
        if self.reads.is_empty() && self.writes.is_empty() {
            return Ok(());
        }
        E::pre_process_memory_accesses(&self.reads, &self.writes)?;
        self.reads.clear();
        self.writes.clear();
        Ok(())
    }

    /// Pre-process registered accesses if need and read data from `memory` to `buff`.
    fn read_into_buf<M: Memory>(
        &mut self,
        memory: &M,
        ptr: u32,
        buff: &mut [u8],
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_memory_accesses()?;
        memory.read(ptr, buff).map_err(Into::into)
    }

    /// Pre-process registered accesses if need and read data from `memory` into new vector.
    pub fn read<M: Memory>(
        &mut self,
        memory: &M,
        read: WasmMemoryRead,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        let mut buff = RuntimeBuffer::try_new_default(read.size as usize)?;
        self.read_into_buf(memory, read.ptr, buff.get_mut())?;
        Ok(buff.into_vec())
    }

    /// Pre-process registered accesses if need and read and decode data as `T` from `memory`.
    pub fn read_decoded<M: Memory, T: Decode + MaxEncodedLen>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        let mut buff = RuntimeBuffer::try_new_default(T::max_encoded_len())?.into_vec();
        self.read_into_buf(memory, read.ptr, &mut buff)?;
        let decoded = T::decode_all(&mut &buff[..]).map_err(|_| MemoryAccessError::DecodeError)?;
        Ok(decoded)
    }

    /// Pre-process registered accesses if need and read data as `T` from `memory`.
    pub fn read_as<M: Memory, T: Sized>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadAs<T>,
    ) -> Result<T, MemoryAccessError> {
        self.pre_process_memory_accesses()?;
        crate::read_memory_as(memory, read.ptr).map_err(Into::into)
    }

    /// Pre-process registered accesses if need and write data from `buff` to `memory`.
    pub fn write<M: Memory>(
        &mut self,
        memory: &mut M,
        write: WasmMemoryWrite,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError> {
        if buff.len() != write.size as usize {
            return Err(MemoryAccessError::WrongBufferSize(buff.len(), write.size));
        }
        self.pre_process_memory_accesses()?;
        memory.write(write.ptr, buff).map_err(Into::into)
    }

    /// Pre-process registered accesses if need and write `obj` data to `memory`.
    pub fn write_as<M: Memory, T: Sized>(
        &mut self,
        memory: &mut M,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_memory_accesses()?;
        crate::write_memory_as(memory, write.ptr, obj).map_err(Into::into)
    }
}

/// Read static size type access wrapper.
pub struct WasmMemoryReadAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Read decoded type access wrapper.
pub struct WasmMemoryReadDecoded<T: Decode + MaxEncodedLen> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Read access wrapper.
pub struct WasmMemoryRead {
    ptr: u32,
    size: u32,
}

/// Write static size type access wrapper.
pub struct WasmMemoryWriteAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

/// Write access wrapper.
pub struct WasmMemoryWrite {
    ptr: u32,
    size: u32,
}
