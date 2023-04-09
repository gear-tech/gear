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

use crate::BackendExt;
use alloc::{collections::BTreeMap, vec::Vec};
use core::{
    fmt::Debug,
    marker::PhantomData,
    mem,
    mem::MaybeUninit,
    slice::{self},
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
    fn register_read(&mut self, ptr: u32, size: u32) -> Result<ReadToken, MemoryAccessError>;

    /// Register new read static size type access.
    fn register_read_as<T: Sized>(&mut self, ptr: u32)
        -> Result<ReadAsToken<T>, MemoryAccessError>;

    /// Register new read decoded type access.
    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> Result<ReadDecodedToken<T>, MemoryAccessError>;
}

pub trait MemoryOwner {
    /// Read from owned memory to new byte vector.
    fn read(&mut self, read: ReadToken) -> Result<Vec<u8>, MemoryAccessError>;

    /// Read from owned memory to new object `T`.
    fn read_as<T: Sized>(&mut self, read: ReadAsToken<T>) -> Result<T, MemoryAccessError>;

    /// Read from owned memory and decoded data into object `T`.
    fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: ReadDecodedToken<T>,
    ) -> Result<T, MemoryAccessError>;

    /// Write data from `buff` to owned memory.
    fn write(&mut self, offset: u32, buff: &[u8]) -> Result<(), MemoryAccessError>;

    /// Write data from `obj` to owned memory.
    fn write_as<T: Sized>(&mut self, offset: u32, obj: T) -> Result<(), MemoryAccessError>;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReadToken {
    interval: MemoryInterval,
    salt: u32,
}

impl ReadToken {
    fn private_clone(&self) -> Self {
        Self {
            interval: self.interval,
            salt: self.salt,
        }
    }
}

#[derive(Debug)]
pub struct ReadAsToken<T> {
    offset: u32,
    salt: u32,
    _phantom: PhantomData<T>,
}

impl<T> From<ReadAsToken<T>> for ReadToken {
    fn from(token: ReadAsToken<T>) -> Self {
        let ReadAsToken { offset, salt, .. } = token;
        let size = u32::try_from(mem::size_of::<T>())
            .unwrap_or_else(|_| unreachable!("Size of `T` is bigger than u32::MAX"));
        Self {
            interval: MemoryInterval { offset, size },
            salt,
        }
    }
}

impl<T> ReadAsToken<T> {
    fn private_clone(&self) -> Self {
        Self {
            offset: self.offset,
            salt: self.salt,
            _phantom: Default::default(),
        }
    }
}

#[derive(Debug)]
/// Read decoded type access wrapper.
pub struct ReadDecodedToken<T: Decode + MaxEncodedLen> {
    offset: u32,
    salt: u32,
    _phantom: PhantomData<T>,
}

impl<T: Decode + MaxEncodedLen> ReadDecodedToken<T> {
    fn private_clone(&self) -> Self {
        Self {
            offset: self.offset,
            salt: self.salt,
            _phantom: Default::default(),
        }
    }
}

impl<T: Decode + MaxEncodedLen> From<ReadDecodedToken<T>> for ReadToken {
    fn from(token: ReadDecodedToken<T>) -> Self {
        let ReadDecodedToken { offset, salt, .. } = token;
        let size = u32::try_from(T::max_encoded_len())
            .unwrap_or_else(|_| unreachable!("Max encoded len of `T` is bigger than u32::MAX"));
        Self {
            interval: MemoryInterval { offset, size },
            salt,
        }
    }
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
    reads: Vec<ReadToken>,
    reads_data: BTreeMap<ReadToken, Vec<u8>>,
    salt: u32,
    _phantom: PhantomData<E>,
}

impl<E> Default for MemoryAccessManager<E> {
    fn default() -> Self {
        Self {
            reads: Default::default(),
            reads_data: Default::default(),
            salt: 0,
            _phantom: PhantomData,
        }
    }
}

impl<E: BackendExt> MemoryAccessRecorder for MemoryAccessManager<E> {
    fn register_read(&mut self, offset: u32, size: u32) -> Result<ReadToken, MemoryAccessError> {
        RuntimeBuffer::check_size(size as usize)?;
        let token = ReadToken {
            interval: MemoryInterval { offset, size },
            salt: self.salt,
        };
        self.salt += 1;
        self.save_read_token(token.private_clone())?;
        Ok(token)
    }

    fn register_read_as<T: Sized>(
        &mut self,
        offset: u32,
    ) -> Result<ReadAsToken<T>, MemoryAccessError> {
        let token = ReadAsToken {
            offset,
            salt: self.salt,
            _phantom: Default::default(),
        };
        self.salt += 1;
        self.save_read_token(token.private_clone().into())?;
        Ok(token)
    }

    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        offset: u32,
    ) -> Result<ReadDecodedToken<T>, MemoryAccessError> {
        let token = ReadDecodedToken {
            offset,
            salt: self.salt,
            _phantom: Default::default(),
        };
        self.salt += 1;
        self.save_read_token(token.private_clone().into())?;
        Ok(token)
    }
}

impl<E: BackendExt> MemoryAccessManager<E> {
    fn save_read_token(&mut self, token: ReadToken) -> Result<(), MemoryAccessError> {
        RuntimeBuffer::check_size(token.interval.size as usize)?;
        self.reads.push(token);
        Ok(())
    }

    fn pre_process_reads<M: Memory>(
        &mut self,
        memory: &M,
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        if self.reads.is_empty() {
            return Ok(());
        }
        let reads: Vec<MemoryInterval> = self.reads.iter().map(|token| token.interval).collect();
        let reads_data = E::access_memory_reads(memory, &reads, gas_left)?;
        if reads.len() != reads_data.len() {
            unreachable!(
                "Wrong memory backend behavior - must return the same amount of reads data buffers"
            );
        }
        self.reads.iter().zip(reads_data.into_iter()).for_each(|(token, data)| {
            if data.len() != token.interval.size as usize {
                unreachable!("Wrong memory backend behavior - must return the same size of data buffer for each read");
            }
            self.reads_data.insert(token.private_clone(), data);
        });
        self.reads.clear();
        Ok(())
    }

    fn pre_process_and_write<M: Memory>(
        &mut self,
        memory: &mut M,
        offset: u32,
        data: &[u8],
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        let size = u32::try_from(data.len())
            .unwrap_or_else(|_| unreachable!("Write buffer size is bigger than u32::MAX"));
        let write = MemoryInterval { offset, size };
        let reads: Vec<MemoryInterval> = self.reads.iter().map(|token| token.interval).collect();
        let reads_data = E::access_memory_with_writes(memory, &reads, &[(write, data)], gas_left)?;
        if reads.len() != reads_data.len() {
            unreachable!(
                "Wrong memory backend behavior - must return the same amount of reads data buffers"
            );
        }
        self.reads.iter().zip(reads_data.into_iter()).for_each(|(token, data)| {
            if data.len() != token.interval.size as usize {
                unreachable!("Wrong memory backend behavior - must return the same size of data buffer for each read");
            }
            self.reads_data.insert(token.private_clone(), data);
        });
        self.reads.clear();
        Ok(())
    }

    /// Pre-process registered accesses if need and read data from `memory` into new vector.
    pub fn read<M: Memory>(
        &mut self,
        memory: &M,
        token: ReadToken,
        gas_left: &mut GasLeft,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        self.pre_process_reads(memory, gas_left)?;
        let data = self
            .reads_data
            .remove(&token)
            .unwrap_or_else(|| unreachable!("Given token is not created by this memory manager"));
        Ok(data)
    }

    /// Pre-process registered accesses if need and read data as `T` from `memory`.
    pub fn read_as<M: Memory, T: Sized>(
        &mut self,
        memory: &M,
        token: ReadAsToken<T>,
        gas_left: &mut GasLeft,
    ) -> Result<T, MemoryAccessError> {
        self.pre_process_reads(memory, gas_left)?;
        let data = self
            .reads_data
            .remove(&token.into())
            .unwrap_or_else(|| unreachable!("Given token is not created by this memory manager"));
        interpret_as(&data)
    }

    /// Pre-process registered accesses if need and read and decode data as `T` from `memory`.
    pub fn read_decoded<M: Memory, T: Decode + MaxEncodedLen>(
        &mut self,
        memory: &M,
        token: ReadDecodedToken<T>,
        gas_left: &mut GasLeft,
    ) -> Result<T, MemoryAccessError> {
        self.pre_process_reads(memory, gas_left)?;
        let data = self
            .reads_data
            .remove(&token.into())
            .unwrap_or_else(|| unreachable!("Given token is not created by this memory manager"));
        T::decode_all(&mut data.as_slice()).map_err(|_| MemoryAccessError::Decode)
    }

    /// Pre-process registered accesses if need and write data from `buff` to `memory`.
    pub fn write<M: Memory>(
        &mut self,
        memory: &mut M,
        offset: u32,
        data: &[u8],
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_and_write(memory, offset, data, gas_left)
    }

    /// Pre-process registered accesses if need and write `obj` data to `memory`.
    pub fn write_as<M: Memory, T: Sized>(
        &mut self,
        memory: &mut M,
        offset: u32,
        obj: T,
        gas_left: &mut GasLeft,
    ) -> Result<(), MemoryAccessError> {
        // # Safety:
        //
        // Given object is `Sized` and we own them in the context of calling this
        // function (it's on stack), it's safe to take ptr on the object and
        // represent it as slice. Object will be dropped after `memory.write`
        // finished execution and no one will rely on this slice.
        //
        // Bytes in memory always stored continuously and without paddings, properly
        // aligned due to `[repr(C, packed)]` attribute of the types we use as T.
        let data =
            unsafe { slice::from_raw_parts(&obj as *const T as *const u8, mem::size_of::<T>()) };
        self.pre_process_and_write(memory, offset, data, gas_left)
    }
}

/// Reads bytes from given pointer to construct type T from them.
fn interpret_as<T: Sized>(data: &[u8]) -> T {
    let mut buf = MaybeUninit::<T>::uninit();

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
    let mut_slice =
        unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, mem::size_of::<T>()) };

    // Panics if `data` has different size.
    mut_slice.copy_from_slice(data);

    // # Safety:
    //
    // Assuming init is always safe here due to the fact that we read proper
    // amount of bytes from the wasm memory, which is never uninited: they may
    // be filled by zeroes or some trash (valid for our primitives used as T),
    // but always exist.
    unsafe { buf.assume_init() }
}
