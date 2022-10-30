use core::{marker::PhantomData, mem::size_of};

use alloc::vec::Vec;
use codec::{Decode, DecodeAll, MaxEncodedLen};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    memory::Memory,
};
use gear_core_errors::MemoryError;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum MemoryAccessError {
    #[from]
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeBuffer(RuntimeBufferSizeError),
    DecodeError,
}

#[derive(Debug)]
pub struct MemoryAccessManager<E: Ext> {
    reads: Vec<(u32, u32)>,
    writes: Vec<(u32, u32)>,
    _phantom: PhantomData<E>,
}

impl<E: Ext> Default for MemoryAccessManager<E> {
    fn default() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<E: Ext> MemoryAccessManager<E> {
    pub fn add_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        self.reads.push((ptr, size));
        WasmMemoryRead { ptr, size }
    }
    pub fn add_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        self.reads.push((ptr, size_of::<T>() as u32));
        WasmMemoryReadAs {
            ptr,
            _phantom: PhantomData,
        }
    }
    pub fn add_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T> {
        self.reads.push((ptr, T::max_encoded_len() as u32));
        WasmMemoryReadDecoded {
            ptr,
            _phantom: PhantomData,
        }
    }
    pub fn add_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        self.writes.push((ptr, size));
        WasmMemoryWrite { ptr, size }
    }
    pub fn add_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        self.writes.push((ptr, size_of::<T>() as u32));
        WasmMemoryWriteAs {
            ptr,
            _phantom: PhantomData,
        }
    }
    fn pre_process_memory_accesses(&mut self) -> Result<(), MemoryAccessError> {
        if self.reads.is_empty() && self.writes.is_empty() {
            return Ok(());
        }
        E::pre_process_memory_accesses(&self.reads, &self.writes)
            .map_err(|_| MemoryError::OutOfBounds)?;
        self.reads.clear();
        self.writes.clear();
        Ok(())
    }
    fn read_into_buff<M: Memory>(
        &mut self,
        memory: &M,
        ptr: u32,
        buff: &mut [u8],
    ) -> Result<(), MemoryAccessError> {
        self.pre_process_memory_accesses()?;
        memory.read(ptr, buff).map_err(Into::into)
    }
    pub fn read<M: Memory>(
        &mut self,
        memory: &M,
        read: WasmMemoryRead,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        let mut buff = RuntimeBuffer::try_new_default(read.size as usize)?;
        self.read_into_buff(memory, read.ptr, buff.get_mut())?;
        Ok(buff.into_vec())
    }
    pub fn read_decoded<M: Memory, T: Decode + MaxEncodedLen>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        let mut buff = RuntimeBuffer::try_new_default(T::max_encoded_len())?.into_vec();
        self.read_into_buff(memory, read.ptr, &mut buff)?;
        let decoded = T::decode_all(&mut &buff[..]).map_err(|_| MemoryAccessError::DecodeError)?;
        Ok(decoded)
    }
    pub fn read_as<M: Memory, T: Sized>(
        &mut self,
        memory: &M,
        read: WasmMemoryReadAs<T>,
    ) -> Result<T, MemoryAccessError> {
        self.pre_process_memory_accesses()?;
        crate::read_memory_as(memory, read.ptr).map_err(Into::into)
    }
    pub fn write<M: Memory>(
        &mut self,
        memory: &mut M,
        write: WasmMemoryWrite,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError> {
        assert_eq!(
            buff.len(),
            write.size as usize,
            "Runtime error: pre-processed size and buff size must be equal"
        );
        self.pre_process_memory_accesses()?;
        memory.write(write.ptr, buff).map_err(Into::into)
    }
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

pub struct WasmMemoryReadAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

pub struct WasmMemoryReadDecoded<T: Decode + MaxEncodedLen> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

pub struct WasmMemoryRead {
    ptr: u32,
    size: u32,
}

pub struct WasmMemoryWriteAs<T> {
    ptr: u32,
    _phantom: PhantomData<T>,
}

pub struct WasmMemoryWrite {
    ptr: u32,
    size: u32,
}
